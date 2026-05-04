use async_trait::async_trait;
use serde_json::{json, Value};
use taskhub_core::{engine::Action, TaskHubError};

pub struct EmailImapAction;

#[async_trait]
impl Action for EmailImapAction {
    fn plugin_id(&self) -> &str { "email_imap" }
    fn action_id(&self) -> &str { "inbox.list" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let host = input["host"].as_str().ok_or_else(|| TaskHubError::Plugin("email_imap: 'host' required".into()))?.to_string();
        let port = input["port"].as_u64().unwrap_or(993) as u16;
        let username = input["username"].as_str().ok_or_else(|| TaskHubError::Plugin("email_imap: 'username' required".into()))?.to_string();
        let password = input["password"].as_str().ok_or_else(|| TaskHubError::Plugin("email_imap: 'password' required".into()))?.to_string();
        let mailbox = input["mailbox"].as_str().unwrap_or("INBOX").to_string();
        let limit = input["limit"].as_u64().unwrap_or(20) as usize;
        let action = input["action"].as_str().unwrap_or("inbox.list").to_string();
        let search_query = input["query"].as_str().unwrap_or("ALL").to_string();
        let uid_str = input["uid"].as_str().map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            imap_op(&host, port, &username, &password, &mailbox, &action, limit, &search_query, uid_str.as_deref())
        })
        .await
        .map_err(|e| TaskHubError::Plugin(e.to_string()))?
        .map_err(|e| TaskHubError::Plugin(e))
    }
}

fn imap_op(
    host: &str, port: u16,
    username: &str, password: &str,
    mailbox: &str,
    action: &str,
    limit: usize,
    search_query: &str,
    uid: Option<&str>,
) -> Result<Value, String> {
    let client = imap::ClientBuilder::new(host, port)
        .connect()
        .map_err(|e| format!("IMAP connect: {e}"))?;
    let mut session = client.login(username, password)
        .map_err(|(e, _)| format!("IMAP login: {e}"))?;

    let mb = session.select(mailbox).map_err(|e| format!("select mailbox: {e}"))?;
    let exists = mb.exists;

    match action {
        "inbox.list" | "inbox.search" => {
            let uids = session.search(search_query).map_err(|e| format!("search: {e}"))?;
            let mut uid_list: Vec<u32> = uids.into_iter().collect();
            uid_list.sort_unstable_by(|a, b| b.cmp(a));
            uid_list.truncate(limit);

            if uid_list.is_empty() {
                session.logout().ok();
                return Ok(json!([]));
            }

            let seq_set: String = uid_list.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
            let messages = session
                .fetch(&seq_set, "(UID FLAGS ENVELOPE BODY.PEEK[TEXT]<0.512>)")
                .map_err(|e| format!("fetch: {e}"))?;

            let mut results = vec![];
            for msg in messages.iter() {
                let uid = msg.uid.unwrap_or(0);
                let envelope = msg.envelope();
                let (subject, from_addr, date) = if let Some(env) = envelope {
                    let subj = env.subject.as_ref()
                        .and_then(|s| String::from_utf8(s.to_vec()).ok())
                        .unwrap_or_default();
                    let from = env.from.as_ref()
                        .and_then(|addrs| addrs.first())
                        .map(|a| {
                            let name = a.name.as_ref().and_then(|n| String::from_utf8(n.to_vec()).ok()).unwrap_or_default();
                            let mailbox = a.mailbox.as_ref().and_then(|m| String::from_utf8(m.to_vec()).ok()).unwrap_or_default();
                            let host_part = a.host.as_ref().and_then(|h| String::from_utf8(h.to_vec()).ok()).unwrap_or_default();
                            if name.is_empty() { format!("{}@{}", mailbox, host_part) } else { name }
                        })
                        .unwrap_or_default();
                    let date = env.date.as_ref()
                        .and_then(|d| String::from_utf8(d.to_vec()).ok())
                        .unwrap_or_default();
                    (subj, from, date)
                } else {
                    (String::new(), String::new(), String::new())
                };

                let seen = msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));
                results.push(json!({
                    "uid": uid,
                    "subject": subject,
                    "from": from_addr,
                    "date": date,
                    "seen": seen,
                }));
            }

            session.logout().ok();
            Ok(json!(results))
        }
        "mark_read" => {
            let uid = uid.ok_or("mark_read: 'uid' required")?;
            session.uid_store(uid, "+FLAGS (\\Seen)").map_err(|e| format!("mark_read: {e}"))?;
            session.logout().ok();
            Ok(json!({"ok": true}))
        }
        "move" => {
            let uid = uid.ok_or("move: 'uid' required")?;
            let dest = mailbox; // reuse mailbox field as destination for move
            session.uid_mv(uid, dest).map_err(|e| format!("move: {e}"))?;
            session.logout().ok();
            Ok(json!({"ok": true}))
        }
        other => {
            session.logout().ok();
            Err(format!("unknown action: {}", other))
        }
    }
}

// Unused but required for impl Action — action routing happens via `action` field in input.
pub struct EmailImapSearchAction;

#[async_trait]
impl Action for EmailImapSearchAction {
    fn plugin_id(&self) -> &str { "email_imap" }
    fn action_id(&self) -> &str { "inbox.search" }
    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        EmailImapAction.execute(input).await
    }
}

pub struct EmailImapMarkReadAction;

#[async_trait]
impl Action for EmailImapMarkReadAction {
    fn plugin_id(&self) -> &str { "email_imap" }
    fn action_id(&self) -> &str { "mark_read" }
    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        EmailImapAction.execute(input).await
    }
}

pub struct EmailImapMoveAction;

#[async_trait]
impl Action for EmailImapMoveAction {
    fn plugin_id(&self) -> &str { "email_imap" }
    fn action_id(&self) -> &str { "move" }
    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        EmailImapAction.execute(input).await
    }
}
