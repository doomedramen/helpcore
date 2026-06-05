use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;

use super::{error::ProviderError, types::{ChatMessage, StreamChunk}};

/// A pinned, boxed, `Send` stream of provider chunks.
pub type ProviderStream =
    Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send + 'static>>;

/// Core trait every AI provider must implement.
///
/// `complete` receives an assembled message list (system prompt already
/// prepended as a `system` role message) and returns a live stream.
/// The stream yields `StreamChunk::delta` items followed by a single
/// `StreamChunk::done` terminal item.
#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn default_model(&self) -> &str;

    async fn complete(
        &self,
        messages: &[ChatMessage],
        model: Option<&str>,
    ) -> Result<ProviderStream, ProviderError>;
}
