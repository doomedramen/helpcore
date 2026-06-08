//! Provider integrations for external AI services (Anthropic, OpenAI-compatible, Ollama, etc.).
//!
//! This module handles building, registering, and routing to chat completion providers
//! based on configured roles.

pub mod anthropic;
pub mod error;
pub mod factory;
pub mod ollama;
pub mod openai_compatible;
pub mod registry;
pub mod reliable;
pub mod traits;
pub mod types;

pub use traits::ProviderStream;
