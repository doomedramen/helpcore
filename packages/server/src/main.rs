use helpcore_server::{api, auth, config, conversation, db, plugins, providers, state};

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

    let mut config = config::Config::load(&config_path).with_context(|| {
        format!(
            "failed to load config from {}. \
             Copy config.toml.example → config.toml and edit it, \
             or set HELPCORE_CONFIG to the path of your config file.",
            config_path.display()
        )
    })?;
    config.validate().context("invalid server configuration")?;

    let data_dir = config.data.resolved_dir();
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create data dir {}", data_dir.display()))?;

    let db_path = data_dir.join("helpcore.db");
    let db = db::DbPool::open(&db_path)?;

    // Migrate provider IDs from user-chosen strings to UUIDs.
    if config_path.is_file() {
        let id_mapping = config::Config::migrate_provider_ids(&config_path)?;
        if !id_mapping.is_empty() {
            tracing::info!(count = id_mapping.len(), "migrated provider IDs to UUIDs");
            db.call_sync(|conn| {
                for (old_id, new_id) in &id_mapping {
                    conn.execute(
                        "UPDATE user_provider_grants SET provider_id = ?1 WHERE provider_id = ?2",
                        rusqlite::params![new_id, old_id],
                    )?;
                    tracing::info!(%old_id, %new_id, "updated provider grants");
                }
                Ok(())
            })?;
            // Reload config as provider IDs have changed.
            config = config::Config::load(&config_path)?;
            config
                .validate()
                .context("invalid server configuration after provider ID migration")?;
        }
    }

    let interrupted = db.call_sync(conversation::history::interrupt_active_messages)?;
    if interrupted > 0 {
        tracing::warn!(
            count = interrupted,
            "marked unfinished responses as interrupted"
        );
    }

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
    db.call_sync(|conn| plugins::registry::load_local_plugins(conn, &config.plugins.local))?;

    let provider_registry = providers::registry::ProviderRegistry::new(
        providers::registry::ProviderRegistry::prepare(&config.providers)
            .context("failed to load configured providers")?,
    );
    for provider in &config.providers {
        tracing::info!(id = provider.id, name = provider.name, "loaded provider");
    }
    if provider_registry.is_empty() {
        tracing::warn!("no providers configured — POST /chat will return an error");
    }

    let state = Arc::new(state::AppState {
        config: Arc::new(config),
        config_path,
        data_dir,
        db: Arc::new(db),
        providers: provider_registry,
        config_update_lock: Arc::new(tokio::sync::Mutex::new(())),
    });

    let port = state.config.server.port;
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind to {addr}"))?;

    tracing::info!(
        addr,
        version = env!("CARGO_PKG_VERSION"),
        "helpcore listening"
    );

    axum::serve(listener, api::router::create(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install shutdown handler");
    tracing::info!("shutdown signal received; draining active HTTP connections");
}
