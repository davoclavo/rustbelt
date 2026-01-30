//! rustbelt CLI
//!
//! Command-line interface for rustbelt providing both MCP mcp
//! functionality and standalone CLI tools.

use clap::{Parser, Subcommand};
use command::{CommandWrapper, execute_analyzer_command, extract_workspace_path};
use rustbelt_server::VERSION;

mod command;
mod repl;

#[derive(Parser)]
#[command(name = "rustbelt")]
#[command(about = "rustbelt MCP Server - power up your Rust development")]
#[command(version = VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server (defaults to stdio mode)
    Mcp {
        /// Use TCP mode instead of default stdio mode
        #[arg(long)]
        tcp: bool,
        /// Host for TCP mode
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port for TCP mode
        #[arg(long, default_value = "3001")]
        port: u16,
    },
    /// Connect to a workspace for interactive queries
    Repl {
        /// Path to the workspace directory
        workspace_path: String,
    },
    /// Run an analyzer task
    Analyzer(#[command(flatten)] CommandWrapper),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Mcp { tcp, host, port } => {
            if tcp {
                // Run in TCP mode
                // Only initialize logging for TCP mode
                tracing_subscriber::fmt::init();
                let addr = format!("{host}:{port}");
                rustbelt_server::serve_tcp(addr).await?;
            } else {
                // Run in stdio mode - recommended for MCP clients (default)
                // No logging as it would interfere with JSON-RPC communication
                rustbelt_server::serve_stdio().await?;
            }
        }
        Commands::Repl { workspace_path } => {
            // Initialize logging for debugging
            tracing_subscriber::fmt::init();

            repl::run_repl(&workspace_path).await?;
        }
        Commands::Analyzer(command_wrapper) => {
            // Initialize logging for debugging
            tracing_subscriber::fmt::init();

            let analyzer_command = command_wrapper.command;
            // For analyzer commands, we need to determine the workspace path
            let workspace_path = extract_workspace_path(&analyzer_command);
            execute_analyzer_command(analyzer_command, &workspace_path).await?;
        }
    }

    Ok(())
}
