use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskHubError {
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    #[error("workflow parse error: {0}")]
    WorkflowParse(String),

    #[error("plugin error: {0}")]
    Plugin(String),

    #[error("secret not found: {0}")]
    SecretNotFound(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
