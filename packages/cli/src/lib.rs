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
    }
}
