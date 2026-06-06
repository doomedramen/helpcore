use std::{sync::Arc, time::Duration};

use async_trait::async_trait;

use super::{
    error::ProviderError,
    traits::{ChatProvider, ProviderStream},
    types::{ChatMessage, ToolDefinition},
};

/// Wraps any `ChatProvider` with retry + exponential backoff.
///
/// - Non-retryable errors (auth, context-too-long) are returned immediately.
/// - Retryable errors (request failures, rate limits, unavailable) are retried
///   up to `max_retries` times with doubling delay starting at `initial_delay`.
pub struct ReliableProvider {
    inner: Arc<dyn ChatProvider>,
    max_retries: u32,
    initial_delay: Duration,
}

impl ReliableProvider {
    pub fn new(inner: Arc<dyn ChatProvider>, max_retries: u32) -> Self {
        Self {
            inner,
            max_retries,
            initial_delay: Duration::from_millis(500),
        }
    }
}

#[async_trait]
impl ChatProvider for ReliableProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn default_model(&self) -> &str {
        self.inner.default_model()
    }
    fn context_limit(&self) -> u32 {
        self.inner.context_limit()
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        model: Option<&str>,
    ) -> Result<ProviderStream, ProviderError> {
        let mut last_err: Option<ProviderError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = self.initial_delay * 2u32.pow((attempt - 1).min(4));
                tracing::warn!(
                    attempt,
                    provider = self.id(),
                    delay_ms = backoff.as_millis(),
                    "retrying provider call"
                );
                tokio::time::sleep(backoff).await;
            }

            match self.inner.complete(messages, tools, model).await {
                Ok(stream) => return Ok(stream),
                Err(e) if !e.is_retryable() => {
                    tracing::warn!(error = %e, provider = self.id(), "non-retryable provider error");
                    return Err(e);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        attempt,
                        max = self.max_retries,
                        provider = self.id(),
                        "provider call failed"
                    );
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.expect("loop must execute at least once"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{
        error::ProviderError,
        traits::{ChatProvider, ProviderStream},
        types::{ChatMessage, StreamChunk},
    };
    use futures_util::{StreamExt, stream};
    use std::sync::atomic::{AtomicU32, Ordering};

    // Mock provider that fails `fail_count` times then succeeds.
    struct FlakyProvider {
        calls: AtomicU32,
        fail_count: u32,
    }

    impl FlakyProvider {
        fn new(fail_count: u32) -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicU32::new(0),
                fail_count,
            })
        }
    }

    #[async_trait]
    impl ChatProvider for FlakyProvider {
        fn id(&self) -> &str {
            "flaky"
        }
        fn name(&self) -> &str {
            "Flaky"
        }
        fn default_model(&self) -> &str {
            "test"
        }
        fn context_limit(&self) -> u32 {
            4096
        }

        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _model: Option<&str>,
        ) -> Result<ProviderStream, ProviderError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_count {
                Err(ProviderError::Unavailable)
            } else {
                let s = stream::iter(vec![Ok(StreamChunk::delta("ok")), Ok(StreamChunk::done())]);
                Ok(Box::pin(s))
            }
        }
    }

    struct AlwaysFailProvider;

    #[async_trait]
    impl ChatProvider for AlwaysFailProvider {
        fn id(&self) -> &str {
            "fail"
        }
        fn name(&self) -> &str {
            "Fail"
        }
        fn default_model(&self) -> &str {
            "test"
        }
        fn context_limit(&self) -> u32 {
            4096
        }
        async fn complete(
            &self,
            _: &[ChatMessage],
            _: &[ToolDefinition],
            _: Option<&str>,
        ) -> Result<ProviderStream, ProviderError> {
            Err(ProviderError::Unavailable)
        }
    }

    fn fast_reliable(inner: Arc<dyn ChatProvider>) -> ReliableProvider {
        ReliableProvider {
            inner,
            max_retries: 3,
            initial_delay: Duration::from_millis(1),
        }
    }

    #[tokio::test]
    async fn succeeds_on_first_try() {
        let inner = FlakyProvider::new(0);
        let reliable = fast_reliable(inner.clone());
        let mut stream = reliable
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await
            .unwrap();
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.delta, "ok");
    }

    #[tokio::test]
    async fn retries_and_eventually_succeeds() {
        let inner = FlakyProvider::new(2); // fail twice, succeed on 3rd
        let reliable = fast_reliable(inner.clone());
        let result = reliable
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await;
        assert!(result.is_ok(), "should succeed after retries");
        assert_eq!(inner.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn exhausts_retries_and_returns_error() {
        let inner = Arc::new(AlwaysFailProvider);
        let reliable = fast_reliable(inner);
        let result = reliable
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn non_retryable_error_fails_immediately() {
        struct AuthFail;
        #[async_trait]
        impl ChatProvider for AuthFail {
            fn id(&self) -> &str {
                "authfail"
            }
            fn name(&self) -> &str {
                "AuthFail"
            }
            fn default_model(&self) -> &str {
                "test"
            }
            fn context_limit(&self) -> u32 {
                4096
            }
            async fn complete(
                &self,
                _: &[ChatMessage],
                _: &[ToolDefinition],
                _: Option<&str>,
            ) -> Result<ProviderStream, ProviderError> {
                Err(ProviderError::AuthenticationFailed)
            }
        }
        let reliable = fast_reliable(Arc::new(AuthFail));
        let result = reliable
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await;
        assert!(matches!(result, Err(ProviderError::AuthenticationFailed)));
    }
}
