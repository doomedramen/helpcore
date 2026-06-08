#![allow(missing_docs)]

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;

use super::{
    error::ProviderError,
    types::{ChatMessage, StreamChunk, ToolDefinition},
};

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

    /// The maximum context window in tokens for this provider/model.
    /// Used to decide when to auto-compact conversation history.
    fn context_limit(&self) -> u32;

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        model: Option<&str>,
    ) -> Result<ProviderStream, ProviderError>;
}
