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
        /// The question to ask
        #[arg(required = true)]
        query: Vec<String>,
    },
    /// Log in to a helpcore server
    Login,
    /// Log out of the current server
    Logout,
    /// Initial server setup wizard
    Setup,
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Ask { .. } => eprintln!("not yet implemented"),
        Commands::Login => eprintln!("not yet implemented"),
        Commands::Logout => eprintln!("not yet implemented"),
        Commands::Setup => eprintln!("not yet implemented"),
    }
}
