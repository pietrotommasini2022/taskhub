pub mod email_imap;
pub mod email_smtp;
pub mod http;
pub mod postgres_plugin;
pub mod shell;
pub mod sqlite_plugin;
pub mod transform;

pub use email_imap::{EmailImapAction, EmailImapMarkReadAction, EmailImapMoveAction, EmailImapSearchAction};
pub use email_smtp::EmailSmtpAction;
pub use http::HttpAction;
pub use postgres_plugin::{PostgresExecuteAction, PostgresQueryAction};
pub use shell::ShellAction;
pub use sqlite_plugin::{SqliteExecuteAction, SqliteQueryAction, SqliteTransactionAction};
pub use transform::TransformAction;

use std::sync::Arc;

pub fn register_all(engine: &mut taskhub_core::Engine) {
    engine.register(Arc::new(HttpAction::new()));
    engine.register(Arc::new(ShellAction));
    engine.register(Arc::new(TransformAction));
    engine.register(Arc::new(EmailSmtpAction));
    engine.register(Arc::new(EmailImapAction));
    engine.register(Arc::new(EmailImapSearchAction));
    engine.register(Arc::new(EmailImapMarkReadAction));
    engine.register(Arc::new(EmailImapMoveAction));
    engine.register(Arc::new(SqliteQueryAction));
    engine.register(Arc::new(SqliteExecuteAction));
    engine.register(Arc::new(SqliteTransactionAction));
    engine.register(Arc::new(PostgresQueryAction));
    engine.register(Arc::new(PostgresExecuteAction));
}
