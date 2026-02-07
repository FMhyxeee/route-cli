mod commands;
mod config;
mod proxy;
mod subscription;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "codex-route",
    version,
    about = "Run Codex CLI with scoped VPN proxy forwarding"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    LoginSub {
        #[arg(long)]
        url: String,
    },
    Update,
    ListNodes,
    UseNode {
        node_name: String,
    },
    Run {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    Doctor,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::LoginSub { url } => commands::cmd_login_sub(url).await.map(|_| 0),
        Commands::Update => commands::cmd_update().await.map(|_| 0),
        Commands::ListNodes => commands::cmd_list_nodes().await.map(|_| 0),
        Commands::UseNode { node_name } => commands::cmd_use_node(node_name).await.map(|_| 0),
        Commands::Run { command } => commands::cmd_run(command).await,
        Commands::Doctor => commands::cmd_doctor().await.map(|_| 0),
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Error: {err:#}");
            std::process::exit(1);
        }
    }
}
