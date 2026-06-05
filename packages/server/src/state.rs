use std::sync::Arc;

use crate::{config::Config, db::DbPool, providers::traits::ChatProvider};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<DbPool>,
    /// Providers in config order. Chat handler picks the first available.
    pub providers: Vec<Arc<dyn ChatProvider>>,
}
