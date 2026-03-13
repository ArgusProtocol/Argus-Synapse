use clap::{Parser, Subcommand};
use tracing::info;
use std::sync::Arc;

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
    /// Check connectivity and health (connects via TCP JSON-RPC)
    Check {
        /// TCP endpoint to check, in host:port format
        #[arg(long, default_value = "127.0.0.1:9293")]
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
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::TcpStream;

            info!("Checking Argus connectivity at {} (TCP JSON-RPC)...", endpoint);

            // The Argus RPC server communicates over raw TCP JSON-RPC 2.0,
            // not HTTP. We replicate what dag_client.py does.
            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "get_health",
                "params": null,
                "id": 1
            });
            let payload_bytes = serde_json::to_vec(&payload)?;

            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                TcpStream::connect(&endpoint),
            )
            .await
            {
                Ok(Ok(mut stream)) => {
                    stream.write_all(&payload_bytes).await?;

                    let mut buf = vec![0u8; 16384];
                    match stream.read(&mut buf).await {
                        Ok(n) if n > 0 => {
                            let body: serde_json::Value =
                                serde_json::from_slice(&buf[..n])?;
                            if let Some(result) = body.get("result") {
                                println!("Argus Gateway: [OK]");
                                println!(
                                    "Agent State:   [{}]",
                                    result
                                        .get("agent_state")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("UNKNOWN")
                                );
                                println!(
                                    "Current K:     [{}]",
                                    result
                                        .get("current_k")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                );
                                println!(
                                    "Total Blocks:  [{}]",
                                    result
                                        .get("total_blocks")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                );
                            } else if let Some(err) = body.get("error") {
                                println!(
                                    "Argus Gateway: [ERROR] - {}",
                                    err.get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Internal Error")
                                );
                            } else {
                                println!("Argus Gateway: [UNEXPECTED RESPONSE] - {:?}", body);
                            }
                        }
                        Ok(_) => {
                            println!("Argus Gateway: [EMPTY RESPONSE]");
                        }
                        Err(e) => {
                            println!("Argus Gateway: [READ ERROR] - {e}");
                        }
                    }
                }
                Ok(Err(e)) => {
                    println!("Argus Gateway: [UNREACHABLE] - {e}");
                }
                Err(_) => {
                    println!("Argus Gateway: [TIMEOUT] - no response within 5s");
                }
            }
        }
    }

    Ok(())
}
