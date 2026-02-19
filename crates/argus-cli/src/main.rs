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

            let mut dag = DagStore::new();
            let genesis_hash = BlockHash::from_byte(0x00);
            dag.add_genesis(BlockHeader::genesis(genesis_hash, 0))?;
            
            let shared_state = Arc::new(ServerState::new(dag, k));
            
            // Perform initial coloring.
            shared_state.recolor_and_broadcast().await?;

            let config = ServerConfig {
                ws_addr: format!("0.0.0.0:{}", ws_port).parse()?,
                rpc_addr: format!("0.0.0.0:{}", rpc_port).parse()?,
            };

            let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

            // Start the combined RPC + WebSocket server.
            start_server(shared_state, config, shutdown_rx).await;
        }
        Commands::Check { endpoint } => {
            info!("Checking Argus connectivity at {}...", endpoint);
            
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?;
            
            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "get_health",
                "params": {},
                "id": 1
            });

            match client.post(&endpoint).json(&payload).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body: serde_json::Value = resp.json().await?;
                        if let Some(result) = body.get("result") {
                            println!("Argus Gateway: [OK]");
                            println!("Agent State:   [{}]", result.get("agent_state").and_then(|v| v.as_str()).unwrap_or("UNKNOWN"));
                            println!("Current K:     [{}]", result.get("current_k").and_then(|v| v.as_u64()).unwrap_or(0));
                            println!("Total Blocks:  [{}]", result.get("total_blocks").and_then(|v| v.as_u64()).unwrap_or(0));
                        } else if let Some(error) = body.get("error") {
                            println!("Argus Gateway: [ERROR] - {}", error.get("message").and_then(|v| v.as_str()).unwrap_or("Internal Error"));
                        }
                    } else {
                        println!("Argus Gateway: [UNREACHABLE] - Status {}", resp.status());
                    }
                }
                Err(e) => {
                    println!("Argus Gateway: [UNREACHABLE] - {}", e);
                }
            }
        }
    }

    Ok(())
}
