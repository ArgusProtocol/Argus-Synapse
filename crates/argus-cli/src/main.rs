use clap::{Parser, Subcommand};
use tracing::{info, error, warn};
use std::sync::Arc;
use tokio::sync::RwLock;

use argus_ghostdag::{DagStore, BlockHash, BlockHeader};
use argus_linearizer::{start_server, ServerConfig, ServerState};
use argus_agent::{GhostDagAgent, RecoveryLoop, RecoveryConfig, channels::{command_channel, event_channel, AgentEvent}};
use argus_node::KaspadClient;

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
        /// Kaspad RPC endpoint
        #[arg(long, default_value = "http://127.0.0.1:16110")]
        kaspad_rpc: String,
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
        Commands::Start { rpc_port, ws_port, k, kaspad_rpc } => {
            info!("Starting Argus Orchestration Layer...");
            info!("RPC Port: {}, WS Port: {}, k: {}", rpc_port, ws_port, k);

            let mut dag = DagStore::new();
            let genesis_hash = BlockHash::from_byte(0x00);
            dag.add_genesis(BlockHeader::genesis(genesis_hash, 0))?;
            
            let shared_state = Arc::new(ServerState::new(dag, k));
            
            // Channels for Agent communication.
            let (cmd_tx, cmd_rx) = command_channel(128);
            let (event_tx, mut event_rx) = event_channel(128);

            // Shutdown signal.
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

            // 1. Kaspad Client.
            let kaspad = KaspadClient::new(kaspad_rpc, shared_state.dag.clone(), cmd_tx.clone());
            let kaspad_shutdown = shutdown_rx.clone();
            tokio::spawn(async move {
                kaspad.run_ingestion_loop(kaspad_shutdown).await;
            });

            // 2. GhostDagAgent.
            let agent = GhostDagAgent::new(shared_state.dag.clone(), Arc::new(kaspad), genesis_hash, k, cmd_rx, event_tx);
            tokio::spawn(async move {
                agent.run().await;
            });

            // 3. Recovery Loop.
            let network_tip = Arc::new(RwLock::new(None));
            let recovery_loop = RecoveryLoop::new(
                shared_state.dag.clone(),
                RecoveryConfig { k, ..Default::default() },
                cmd_tx,
                event_tx.clone(),
                network_tip.clone(),
                shutdown_rx.clone(),
            );
            tokio::spawn(async move {
                recovery_loop.run().await;
            });

            // Event handler for agent state updates.
            let state_for_events = shared_state.clone();
            tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    match event {
                        AgentEvent::StateChanged { to, .. } => {
                            let mut state_label = state_for_events.agent_state.write().await;
                            *state_label = to.to_string();
                        }
                        AgentEvent::DivergenceDetected { network_tip: tip, .. } => {
                            let mut nt = network_tip.write().await;
                            *nt = Some(tip);
                        }
                        _ => {}
                    }
                }
            });

            // Perform initial coloring.
            shared_state.recolor_and_broadcast().await?;

            let config = ServerConfig {
                ws_addr = format!("0.0.0.0:{}", ws_port).parse()?,
                rpc_addr = format!("0.0.0.0:{}", rpc_port).parse()?,
            };

            // Start the combined RPC + WebSocket server.
            let server_handle = tokio::spawn(start_server(shared_state, config, shutdown_rx));

            // Graceful shutdown on Ctrl+C.
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Shutdown signal received (Ctrl+C)");
                    let _ = shutdown_tx.send(true);
                }
                _ = server_handle => {
                    warn!("Server exited unexpectedly");
                }
            }

            info!("Argus Layer shutting down...");
        }
        Commands::Check { endpoint } => {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::TcpStream;

            // Strip http:// or tcp:// prefix if present.
            let cleaned_endpoint = endpoint
                .strip_prefix("http://")
                .or_else(|| endpoint.strip_prefix("tcp://"))
                .unwrap_or(&endpoint);

            info!("Checking Argus connectivity at {} (TCP JSON-RPC)...", cleaned_endpoint);

            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "get_health",
                "params": null,
                "id": 1
            });
            let payload_bytes = serde_json::to_vec(&payload)?;

            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                TcpStream::connect(cleaned_endpoint),
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
