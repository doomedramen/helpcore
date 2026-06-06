use std::{path::PathBuf, sync::Arc};

use crate::{config::Config, db::DbPool, providers::registry::ProviderRegistry};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub db: Arc<DbPool>,
    pub providers: ProviderRegistry,
    pub config_update_lock: Arc<tokio::sync::Mutex<()>>,
}
