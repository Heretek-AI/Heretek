use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("model endpoint returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("failed to parse model response: {0}")]
    Parse(String),
    #[error("invalid model configuration: {0}")]
    Config(String),
    #[error("missing API key environment variable {0}")]
    MissingApiKey(String),
    #[error("transport error: {0}")]
    Transport(String),
}

impl ModelError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(error) => error.is_timeout() || error.is_connect(),
            Self::Status { status, .. } => *status == 429 || (500..=599).contains(status),
            Self::Parse(_) | Self::Config(_) | Self::MissingApiKey(_) | Self::Transport(_) => false,
        }
    }
}
