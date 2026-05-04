use async_trait::async_trait;
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde_json::Value;
use taskhub_core::{engine::Action, TaskHubError};

pub struct EmailSmtpAction;

#[async_trait]
impl Action for EmailSmtpAction {
    fn plugin_id(&self) -> &str { "email_smtp" }
    fn action_id(&self) -> &str { "send" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let host = input["host"].as_str().ok_or_else(|| TaskHubError::Plugin("email_smtp/send: 'host' required".into()))?;
        let port = input["port"].as_u64().unwrap_or(587) as u16;
        let username = input["username"].as_str().ok_or_else(|| TaskHubError::Plugin("email_smtp/send: 'username' required".into()))?;
        let password = input["password"].as_str().ok_or_else(|| TaskHubError::Plugin("email_smtp/send: 'password' required".into()))?;
        let from = input["from"].as_str().ok_or_else(|| TaskHubError::Plugin("email_smtp/send: 'from' required".into()))?;
        let to = input["to"].as_str().ok_or_else(|| TaskHubError::Plugin("email_smtp/send: 'to' required".into()))?;
        let subject = input["subject"].as_str().unwrap_or("(no subject)");
        let body_text = input["body"].as_str().unwrap_or("");
        let body_html = input["body_html"].as_str();

        let from_mb: Mailbox = from.parse().map_err(|e| TaskHubError::Plugin(format!("invalid from: {e}")))?;
        let to_mb: Mailbox = to.parse().map_err(|e| TaskHubError::Plugin(format!("invalid to: {e}")))?;

        let msg_builder = Message::builder()
            .from(from_mb)
            .to(to_mb)
            .subject(subject);

        let message = if let Some(html) = body_html {
            msg_builder.multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::builder().header(ContentType::TEXT_PLAIN).body(body_text.to_string()))
                    .singlepart(SinglePart::builder().header(ContentType::TEXT_HTML).body(html.to_string())),
            )
        } else {
            msg_builder.body(body_text.to_string())
        }
        .map_err(|e| TaskHubError::Plugin(format!("build message: {e}")))?;

        let creds = Credentials::new(username.to_string(), password.to_string());
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| TaskHubError::Plugin(format!("smtp relay: {e}")))?
            .port(port)
            .credentials(creds)
            .build();

        mailer.send(message).await
            .map_err(|e| TaskHubError::Plugin(format!("smtp send: {e}")))?;

        Ok(serde_json::json!({"ok": true}))
    }
}
