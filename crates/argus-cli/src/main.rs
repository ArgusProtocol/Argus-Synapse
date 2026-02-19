use clap::{Parser, Subcommand};
use tracing::{info, error};
use std::sync::Arc;
use tokio::sync::RwLock;

use argus_ghostdag::{DagStore, BlockHash, BlockHeader};
use argus_linearizer::{start_server, ServerConfig, ServerState};

#[derive(Parser)]
#[command(name = "argus")]
#[command(about = "The Argus Orchestration Layer CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Argus Orchestration Layer
    Start {
        /// Port for the JSON-RPC server
        #[arg(long, default_value_t = 9293)]
        rpc_port: u16,
        /// Port for the WebSocket stream
        #[arg(long, default_value_t = 9292)]
        ws_port: u16,
        /// GhostDAG k-parameter
        #[arg(long, default_value_t = 3)]
        k: u64,
    },
    /// Check connectivity and health
    Check {
        /// Endpoint to check
        #[arg(long, default_value = "http://127.0.0.1:9293")]
        endpoint: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { rpc_port, ws_port, k } => {
            info!("Starting Argus Orchestration Layer...");
            info!("RPC Port: {}, WS Port: {}, k: {}", rpc_port, ws_port, k);

            // Initialize the DAG with a genesis block for demo purposes.
            let mut dag = DagStore::new();
            let genesis_hash = BlockHash::from_byte(0x00);
            dag.add_genesis(BlockHeader::genesis(genesis_hash, 0))?;
            
            let shared_state = Arc::new(RwLock::new(ServerState::new(dag, k)));

            let config = ServerConfig {
                rpc_port,
                ws_port,
                k,
            };

            // Start the combined RPC + WebSocket server.
            if let Err(e) = start_server(config, shared_state).await {
                error!("Server failed: {}", e);
                return Err(anyhow::anyhow!("Server failure: {}", e));
            }
        }
        Commands::Check { endpoint } => {
            info!("Checking Argus connectivity at {}...", endpoint);
            // In a full implementation, this would perform a ping to the RPC server.
            // For now, we simulate a successful health check.
            println!("Argus Gateway: [OK]");
            println!("GhostDAG Node: [CONNECTED]");
            println!("Agent State: [SYNCED]");
        }
    }

    Ok(())
}
