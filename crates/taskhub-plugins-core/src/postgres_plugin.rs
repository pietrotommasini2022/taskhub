use async_trait::async_trait;
use serde_json::{json, Map, Value};
use taskhub_core::{engine::Action, TaskHubError};
use tokio_postgres::{types::Type, NoTls};

async fn connect(conn_str: &str) -> Result<tokio_postgres::Client, String> {
    let (client, connection) = tokio_postgres::connect(conn_str, NoTls)
        .await
        .map_err(|e| format!("postgres connect: {e}"))?;
    tokio::spawn(async move { connection.await.ok(); });
    Ok(client)
}

fn pg_to_json(val: &tokio_postgres::Row, col: usize) -> Value {
    use tokio_postgres::types::FromSql;
    let col_type = val.columns()[col].type_();
    // Try common types in order.
    match col_type {
        &Type::BOOL => val.get::<_, Option<bool>>(col).map(|v| json!(v)).unwrap_or(Value::Null),
        &Type::INT2 | &Type::INT4 => val.get::<_, Option<i32>>(col).map(|v| json!(v)).unwrap_or(Value::Null),
        &Type::INT8 => val.get::<_, Option<i64>>(col).map(|v| json!(v)).unwrap_or(Value::Null),
        &Type::FLOAT4 | &Type::FLOAT8 => val.get::<_, Option<f64>>(col).map(|v| json!(v)).unwrap_or(Value::Null),
        &Type::TEXT | &Type::VARCHAR | &Type::BPCHAR | &Type::NAME => {
            val.get::<_, Option<String>>(col).map(|v| json!(v)).unwrap_or(Value::Null)
        }
        _ => {
            // Fallback: try as text
            val.try_get::<_, String>(col).map(|v| json!(v)).unwrap_or(Value::Null)
        }
    }
}

pub struct PostgresQueryAction;

#[async_trait]
impl Action for PostgresQueryAction {
    fn plugin_id(&self) -> &str { "postgres" }
    fn action_id(&self) -> &str { "query" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let conn_str = input["connection"].as_str()
            .ok_or_else(|| TaskHubError::Plugin("postgres: 'connection' required".into()))?;
        let sql = input["sql"].as_str()
            .ok_or_else(|| TaskHubError::Plugin("postgres: 'sql' required".into()))?;

        let client = connect(conn_str).await.map_err(|e| TaskHubError::Plugin(e))?;
        let rows = client.query(sql, &[]).await.map_err(|e| TaskHubError::Plugin(format!("query: {e}")))?;

        let results: Vec<Value> = rows.iter().map(|row| {
            let mut obj = Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                obj.insert(col.name().to_string(), pg_to_json(row, i));
            }
            Value::Object(obj)
        }).collect();

        Ok(json!(results))
    }
}

pub struct PostgresExecuteAction;

#[async_trait]
impl Action for PostgresExecuteAction {
    fn plugin_id(&self) -> &str { "postgres" }
    fn action_id(&self) -> &str { "execute" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let conn_str = input["connection"].as_str()
            .ok_or_else(|| TaskHubError::Plugin("postgres: 'connection' required".into()))?;
        let sql = input["sql"].as_str()
            .ok_or_else(|| TaskHubError::Plugin("postgres: 'sql' required".into()))?;

        let client = connect(conn_str).await.map_err(|e| TaskHubError::Plugin(e))?;
        let n = client.execute(sql, &[]).await.map_err(|e| TaskHubError::Plugin(format!("execute: {e}")))?;

        Ok(json!({"rows_affected": n}))
    }
}
