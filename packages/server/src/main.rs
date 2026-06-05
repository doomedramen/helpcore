use helpcore_server::{api, auth, config, db, plugins, providers, state};

use anyhow::Context;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config_path = config::Config::config_path();
    tracing::info!(path = %config_path.display(), "loading config");

    // Guard against the docker-compose bind-mount foot-gun: if the host file
    // doesn't exist Docker creates a *directory* at the mount path, which
    // gives an opaque "is a directory" read error.  Surface it clearly.
    if config_path.is_dir() {
        anyhow::bail!(
            "{} is a directory, not a file.\n\
             If you are using Docker, make sure you have created config.toml \
             (cp config.toml.example config.toml) before running `docker compose up`.",
            config_path.display()
        );
    }

    let config = config::Config::load(&config_path).with_context(|| {
        format!(
            "failed to load config from {}. \
             Copy config.toml.example → config.toml and edit it, \
             or set HELPCORE_CONFIG to the path of your config file.",
            config_path.display()
        )
    })?;

    let data_dir = config.data.resolved_dir();
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create data dir {}", data_dir.display()))?;

    let db_path = data_dir.join("helpcore.db");
    let db = db::DbPool::open(&db_path)?;

    // First-run check.
    let server_url = config.server.url.trim_end_matches('/').to_string();
    db.call_sync(|conn| {
        if auth::setup::needs_setup(conn)? {
            let token = auth::setup::generate_setup_token(conn)?;
            let url = format!("{server_url}/setup?token={token}");
            println!("\n  Setup required → {url}");
            println!("  This URL expires in 15 minutes.\n");
            tracing::info!("setup token generated — see stdout for URL");
        }
        Ok(())
    })?;

    // Load local plugins (registers them in DB for all users).
    db.call_sync(|conn| {
        plugins::registry::load_local_plugins(conn, &config.plugins.local)
    })?;

    // Load providers from config.
    let mut provider_list = Vec::new();
    for pc in &config.providers {
        match providers::factory::build(pc) {
            Ok(p) => {
                tracing::info!(id = pc.id, name = pc.name, "loaded provider");
                provider_list.push(p);
            }
            Err(e) => {
                tracing::error!(id = pc.id, error = %e, "failed to load provider — skipping");
            }
        }
    }
    if provider_list.is_empty() {
        tracing::warn!("no providers configured — POST /chat will return an error");
    }

    let state = Arc::new(state::AppState {
        config: Arc::new(config),
        db: Arc::new(db),
        providers: provider_list,
    });

    let port = state.config.server.port;
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind to {addr}"))?;

    tracing::info!(addr, version = env!("CARGO_PKG_VERSION"), "helpcore listening");

    axum::serve(listener, api::router::create(state))
        .await
        .context("server error")?;

    Ok(())
}
