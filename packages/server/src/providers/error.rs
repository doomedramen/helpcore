/// Errors that can occur during a provider call.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("request failed: {0}")]
    Request(String),

    #[error("empty response")]
    Empty,

    #[error("rate limited — back off and retry")]
    RateLimited,

    #[error("authentication failed — check API key")]
    AuthenticationFailed,

    #[error("context too long for this model")]
    ContextTooLong,

    #[error("provider unavailable")]
    Unavailable,
}

impl ProviderError {
    /// Whether this error class is worth retrying.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::Request(_) | ProviderError::RateLimited | ProviderError::Unavailable
        )
    }
}
