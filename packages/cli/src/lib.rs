mod client;
mod commands;
pub mod config;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "helpcore", version, about = "helpcore — personal AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ask the AI a question (one-shot)
    Ask {
        /// Question (multiple words accepted without quotes)
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// Continue an existing conversation
        #[arg(short = 'c', long = "conversation")]
        conversation_id: Option<String>,
        /// Provider to use
        #[arg(short = 'p', long)]
        provider: Option<String>,
        /// Model to use
        #[arg(short = 'm', long)]
        model: Option<String>,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },

    /// Log in to a helpcore server
    Login {
        /// Server base URL
        #[arg(long)]
        server: Option<String>,
        /// Email address (will prompt if omitted)
        #[arg(long)]
        email: Option<String>,
    },

    /// Log out of the current server
    Logout,

    /// First-admin setup wizard
    Setup {
        /// Full setup URL printed by the server on first start
        /// (e.g. http://localhost:3000/setup?token=abc123)
        #[arg(long)]
        url: Option<String>,
        /// Server base URL (alternative to --url when pasting token separately)
        #[arg(long)]
        server: Option<String>,
    },

    /// Show connection status
    Status,

    // ── Compact ───────────────────────────────────────────────────────────────

    /// Compact a conversation by summarising its oldest messages
    Compact {
        /// Conversation to compact (required — use the ID shown after `hc ask`)
        #[arg(short = 'c', long = "conversation")]
        conversation_id: String,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },

    // ── Personality ───────────────────────────────────────────────────────────

    /// Show or update the assistant's soul (tone, values, communication style)
    Soul {
        /// Write this text directly (instead of printing or editing)
        #[arg(long, value_name = "TEXT")]
        set: Option<String>,
        /// Open $EDITOR with the current content
        #[arg(long)]
        edit: bool,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },

    /// Show or update the assistant's identity (who it is for you)
    Identity {
        /// Write this text directly
        #[arg(long, value_name = "TEXT")]
        set: Option<String>,
        /// Open $EDITOR with the current content
        #[arg(long)]
        edit: bool,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },

    /// Show or update your user profile (facts about you the AI should know)
    Me {
        /// Write this text directly
        #[arg(long, value_name = "TEXT")]
        set: Option<String>,
        /// Open $EDITOR with the current content
        #[arg(long)]
        edit: bool,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },

    // ── Memory ────────────────────────────────────────────────────────────────

    /// Manage memory files
    #[command(subcommand)]
    Memory(MemoryCommands),

    // ── Plugins ───────────────────────────────────────────────────────────────

    /// Manage plugins
    #[command(subcommand)]
    Plugin(PluginCommands),

    // ── API keys ──────────────────────────────────────────────────────────────

    /// Manage API keys
    #[command(subcommand)]
    ApiKeys(ApiKeyCommands),
}

#[derive(Subcommand)]
enum ApiKeyCommands {
    /// List your API keys
    Ls {
        #[arg(long)]
        server: Option<String>,
    },
    /// Create a new API key (prints the full key — store it securely)
    Create {
        /// A memorable name for this key (e.g. "laptop CLI")
        name: String,
        #[arg(long)]
        server: Option<String>,
    },
    /// Revoke an API key by ID
    Revoke {
        /// Key ID (shown in `hc api-keys ls`)
        id: String,
        #[arg(long)]
        server: Option<String>,
    },
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// List all memory files
    Ls {
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },
    /// Print a memory file
    Get {
        /// Relative path (e.g. notes.md or home/devices.md)
        path: String,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },
    /// Create or overwrite a memory file (reads stdin if --content is omitted)
    Set {
        /// Relative path
        path: String,
        /// Content to write
        #[arg(long, value_name = "TEXT")]
        content: Option<String>,
        /// Open $EDITOR with current content
        #[arg(long)]
        edit: bool,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },
    /// Delete a memory file
    Rm {
        /// Relative path
        path: String,
        /// Server URL (overrides stored credentials)
        #[arg(long)]
        server: Option<String>,
    },
}

#[derive(Subcommand)]
enum PluginCommands {
    /// List installed plugins
    Ls {
        #[arg(long)]
        server: Option<String>,
    },
    /// Generate a scoped bearer token for a bridge plugin
    Token {
        /// Plugin ID (e.g. home-assistant-bridge)
        plugin_id: String,
        /// Permissions to grant (space-separated, e.g. outbound_http)
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
        #[arg(long)]
        server: Option<String>,
    },
    /// Enable a plugin
    Enable {
        plugin_id: String,
        #[arg(long)]
        server: Option<String>,
    },
    /// Disable a plugin (keeps it installed)
    Disable {
        plugin_id: String,
        #[arg(long)]
        server: Option<String>,
    },
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Ask { query, conversation_id, provider, model, server } => {
            commands::ask::run(
                &query,
                conversation_id.as_deref(),
                provider.as_deref(),
                model.as_deref(),
                server.as_deref(),
            )
            .await
        }

        Commands::Login { server, email } => {
            commands::login::run(server.as_deref(), email.as_deref()).await
        }

        Commands::Logout => commands::logout::run().await,

        Commands::Setup { url, server } => {
            commands::setup::run(url.as_deref(), server.as_deref()).await
        }

        Commands::Status => {
            let creds = config::Credentials::load()?;
            match creds.server_url.as_deref() {
                Some(url) => {
                    let status = if creds.is_logged_in() { "logged in" } else { "not logged in" };
                    println!("Server: {url}");
                    println!("Status: {status}");
                }
                None => println!("Not configured. Run 'helpcore login' to connect to a server."),
            }
            Ok(())
        }

        Commands::Soul { set, edit, server } => {
            commands::memory::personality("soul", set.as_deref(), edit, server.as_deref()).await
        }

        Commands::Identity { set, edit, server } => {
            commands::memory::personality("identity", set.as_deref(), edit, server.as_deref()).await
        }

        Commands::Me { set, edit, server } => {
            commands::memory::personality("user", set.as_deref(), edit, server.as_deref()).await
        }

        Commands::Compact { conversation_id, server } => {
            let creds = config::Credentials::load()?;
            let srv = creds
                .resolve_server(server.as_deref())
                .ok_or_else(|| anyhow::anyhow!("no server configured — run 'hc login' first"))?;
            let token = creds
                .access_token
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("not logged in — run 'hc login' first"))?
                .to_string();
            let client = client::Client::new(&srv);
            let result = client.compact(&token, &conversation_id).await?;
            if result.messages_compacted == 0 {
                eprintln!("Conversation is short enough — nothing to compact.");
            } else {
                eprintln!(
                    "✓ Compacted {} messages into a {}-character summary.",
                    result.messages_compacted, result.summary_length
                );
            }
            Ok(())
        }

        Commands::Plugin(sub) => match sub {
            PluginCommands::Ls { server } => {
                commands::plugin::list(server.as_deref()).await
            }
            PluginCommands::Token { plugin_id, permissions, server } => {
                commands::plugin::token(&plugin_id, permissions, server.as_deref()).await
            }
            PluginCommands::Enable { plugin_id, server } => {
                commands::plugin::enable(&plugin_id, true, server.as_deref()).await
            }
            PluginCommands::Disable { plugin_id, server } => {
                commands::plugin::enable(&plugin_id, false, server.as_deref()).await
            }
        },

        Commands::Memory(sub) => match sub {
            MemoryCommands::Ls { server } => {
                commands::memory::memory_ls(server.as_deref()).await
            }
            MemoryCommands::Get { path, server } => {
                commands::memory::memory_get(&path, server.as_deref()).await
            }
            MemoryCommands::Set { path, content, edit, server } => {
                commands::memory::memory_set(&path, content.as_deref(), edit, server.as_deref())
                    .await
            }
            MemoryCommands::Rm { path, server } => {
                commands::memory::memory_rm(&path, server.as_deref()).await
            }
        },

        Commands::ApiKeys(sub) => match sub {
            ApiKeyCommands::Ls { server } => {
                commands::api_key::list(server.as_deref()).await
            }
            ApiKeyCommands::Create { name, server } => {
                commands::api_key::create(&name, server.as_deref()).await
            }
            ApiKeyCommands::Revoke { id, server } => {
                commands::api_key::revoke(&id, server.as_deref()).await
            }
        },
    }
}
