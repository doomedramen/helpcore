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

    #[error("model not found")]
    ModelNotFound,

    #[error("invalid provider request: {0}")]
    InvalidRequest(String),

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

pub fn http_error(status: reqwest::StatusCode, body: &str) -> ProviderError {
    let lower = body.to_lowercase();
    match status.as_u16() {
        401 | 403 => ProviderError::AuthenticationFailed,
        404 => ProviderError::ModelNotFound,
        408 | 409 => ProviderError::Request(format!("HTTP {status}: {body}")),
        413 => ProviderError::ContextTooLong,
        429 => ProviderError::RateLimited,
        400 if lower.contains("context")
            || lower.contains("maximum context")
            || lower.contains("too many tokens") =>
        {
            ProviderError::ContextTooLong
        }
        400 | 422 => ProviderError::InvalidRequest(format!("HTTP {status}: {body}")),
        500 | 502 | 503 | 504 | 529 => ProviderError::Unavailable,
        _ => ProviderError::Request(format!("HTTP {status}: {body}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_http_errors() {
        assert!(matches!(
            http_error(reqwest::StatusCode::UNAUTHORIZED, ""),
            ProviderError::AuthenticationFailed
        ));
        assert!(matches!(
            http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, ""),
            ProviderError::RateLimited
        ));
        assert!(matches!(
            http_error(
                reqwest::StatusCode::BAD_REQUEST,
                "maximum context length exceeded"
            ),
            ProviderError::ContextTooLong
        ));
        assert!(matches!(
            http_error(reqwest::StatusCode::SERVICE_UNAVAILABLE, ""),
            ProviderError::Unavailable
        ));
    }
}
