use std::{path::PathBuf, sync::Arc};

use crate::{config::Config, db::DbPool, providers::registry::ProviderRegistry};

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Parsed server configuration.
    pub config: Arc<Config>,
    /// Path to the active configuration file.
    pub config_path: PathBuf,
    /// Root directory for server data (DB, plugins, user workspace).
    pub data_dir: PathBuf,
    /// Shared SQLite connection pool.
    pub db: Arc<DbPool>,
    /// Registry of available provider backends.
    pub providers: ProviderRegistry,
    /// Serialises config reload requests to prevent races.
    pub config_update_lock: Arc<tokio::sync::Mutex<()>>,
}
