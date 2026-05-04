use async_trait::async_trait;
use rusqlite::{params_from_iter, Connection};
use serde_json::{json, Map, Value};
use taskhub_core::{engine::Action, TaskHubError};

fn connect(path: &str) -> Result<Connection, String> {
    Connection::open(path).map_err(|e| format!("sqlite open: {e}"))
}

fn rows_to_json(stmt: &mut rusqlite::Statement, param_values: &[Value]) -> Result<Value, String> {
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let params: Vec<rusqlite::types::Value> = param_values.iter().map(json_to_rusqlite).collect();
    let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
        let mut obj = Map::new();
        for (i, name) in col_names.iter().enumerate() {
            let val: rusqlite::types::Value = row.get(i)?;
            obj.insert(name.clone(), rusqlite_to_json(val));
        }
        Ok(Value::Object(obj))
    }).map_err(|e| format!("query: {e}"))?;

    let mut results = vec![];
    for row in rows {
        results.push(row.map_err(|e| format!("row: {e}"))?);
    }
    Ok(Value::Array(results))
}

fn json_to_rusqlite(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { rusqlite::types::Value::Integer(i) }
            else if let Some(f) = n.as_f64() { rusqlite::types::Value::Real(f) }
            else { rusqlite::types::Value::Null }
        }
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

fn rusqlite_to_json(v: rusqlite::types::Value) -> Value {
    match v {
        rusqlite::types::Value::Null => Value::Null,
        rusqlite::types::Value::Integer(i) => json!(i),
        rusqlite::types::Value::Real(f) => json!(f),
        rusqlite::types::Value::Text(s) => Value::String(s),
        rusqlite::types::Value::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
    }
}

macro_rules! sqlite_action {
    ($name:ident, $plugin:literal, $action:literal) => {
        pub struct $name;

        #[async_trait]
        impl Action for $name {
            fn plugin_id(&self) -> &str { $plugin }
            fn action_id(&self) -> &str { $action }

            async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
                let path = input["path"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("sqlite: 'path' required".into()))?
                    .to_string();
                let sql = input["sql"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("sqlite: 'sql' required".into()))?
                    .to_string();
                let empty = vec![];
                let params = input["params"].as_array().unwrap_or(&empty).clone();
                let action_name = $action;

                tokio::task::spawn_blocking(move || -> Result<Value, String> {
                    let conn = connect(&path)?;
                    match action_name {
                        "query" => {
                            let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
                            rows_to_json(&mut stmt, &params)
                        }
                        "execute" => {
                            let param_vals: Vec<rusqlite::types::Value> = params.iter().map(json_to_rusqlite).collect();
                            let affected = conn.execute(&sql, params_from_iter(param_vals.iter()))
                                .map_err(|e| format!("execute: {e}"))?;
                            Ok(json!({"rows_affected": affected}))
                        }
                        _ => Err(format!("unknown sqlite action: {}", action_name)),
                    }
                })
                .await
                .map_err(|e| TaskHubError::Plugin(e.to_string()))?
                .map_err(|e| TaskHubError::Plugin(e))
            }
        }
    };
}

sqlite_action!(SqliteQueryAction, "sqlite", "query");
sqlite_action!(SqliteExecuteAction, "sqlite", "execute");

pub struct SqliteTransactionAction;

#[async_trait]
impl Action for SqliteTransactionAction {
    fn plugin_id(&self) -> &str { "sqlite" }
    fn action_id(&self) -> &str { "transaction" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let path = input["path"].as_str()
            .ok_or_else(|| TaskHubError::Plugin("sqlite/transaction: 'path' required".into()))?
            .to_string();
        let stmts = input["statements"].as_array()
            .ok_or_else(|| TaskHubError::Plugin("sqlite/transaction: 'statements' array required".into()))?
            .clone();

        tokio::task::spawn_blocking(move || -> Result<Value, String> {
            let mut conn = connect(&path)?;
            let tx = conn.transaction().map_err(|e| format!("begin transaction: {e}"))?;
            let mut total_affected = 0usize;
            for stmt_val in &stmts {
                let sql = stmt_val["sql"].as_str().ok_or("each statement needs 'sql'")?;
                let empty = vec![];
                let params = stmt_val["params"].as_array().unwrap_or(&empty);
                let param_vals: Vec<rusqlite::types::Value> = params.iter().map(json_to_rusqlite).collect();
                let n = tx.execute(sql, params_from_iter(param_vals.iter()))
                    .map_err(|e| format!("statement error: {e}"))?;
                total_affected += n;
            }
            tx.commit().map_err(|e| format!("commit: {e}"))?;
            Ok(json!({"rows_affected": total_affected}))
        })
        .await
        .map_err(|e| TaskHubError::Plugin(e.to_string()))?
        .map_err(|e| TaskHubError::Plugin(e))
    }
}
