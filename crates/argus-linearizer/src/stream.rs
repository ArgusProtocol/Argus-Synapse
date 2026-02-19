/// Argus Linearizer — High-velocity WebSocket producer.
///
/// Flattens the 3D Web-DAG into a 1D JSON stream of `LinearizedBlock`
/// frames, pushed to all connected WebSocket clients whenever new blocks
/// are added to the DAG.
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use argus_ghostdag::coloring::ColoringOutput;
use argus_ghostdag::dag::DagStore;
use argus_ghostdag::ordering::total_order;

use crate::schema::LinearizedBlock;

/// A message broadcast to all WebSocket clients.
#[derive(Debug, Clone)]
pub struct StreamFrame {
    /// JSON-encoded `Vec<LinearizedBlock>`.
    pub json: String,
}

/// Convert the current DAG state into a vector of `LinearizedBlock`.
pub fn linearize_dag(
    dag: &DagStore,
    coloring: &ColoringOutput,
) -> Result<Vec<LinearizedBlock>, argus_ghostdag::GhostDagError> {
    let ordered = total_order(dag)?;

    let blocks: Vec<LinearizedBlock> = ordered
        .into_iter()
        .map(|entry| {
            let hdr = dag.get(&entry.hash).expect("block must exist after ordering");
            LinearizedBlock {
                hash: entry.hash.to_hex(),
                blue_score: hdr.blue_score,
                blue_work: hdr.blue_work.to_string(),
                topological_index: entry.topological_index,
                adjacency_list: hdr.parents.iter().map(|p| p.to_hex()).collect(),
                is_blue: coloring.blue_set.contains(&entry.hash),
                selected_parent: hdr.selected_parent.map(|sp| sp.to_hex()),
            }
        })
        .collect();

    Ok(blocks)
}

/// The WebSocket stream producer.
///
/// Accepts incoming WebSocket connections and broadcasts linearized
/// DAG updates to all connected clients.
pub struct WsProducer {
    /// Address to bind.
    addr: SocketAddr,
    /// Broadcast sender for stream frames.
    broadcast_tx: broadcast::Sender<StreamFrame>,
}

impl WsProducer {
    /// Create a new producer bound to the given address.
    /// Returns the producer and a broadcast sender that can be used
    /// to push new frames.
    pub fn new(addr: SocketAddr) -> (Self, broadcast::Sender<StreamFrame>) {
        let (tx, _) = broadcast::channel::<StreamFrame>(256);
        let producer = Self {
            addr,
            broadcast_tx: tx.clone(),
        };
        (producer, tx)
    }

    /// Start accepting WebSocket connections.
    /// This runs until the provided cancellation token is triggered.
    pub async fn run(self, shutdown: tokio::sync::watch::Receiver<bool>) {
        let listener = match TcpListener::bind(self.addr).await {
            Ok(l) => {
                info!(addr = %self.addr, "WebSocket producer listening");
                l
            }
            Err(e) => {
                error!(addr = %self.addr, "Failed to bind WebSocket listener: {e}");
                return;
            }
        };

        let mut shutdown = shutdown;

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer)) => {
                            info!(peer = %peer, "New WebSocket connection");
                            let rx = self.broadcast_tx.subscribe();
                            tokio::spawn(handle_ws_client(stream, peer, rx));
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {e}");
                        }
                    }
                }
                Ok(()) = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("WebSocket producer shutting down");
                        break;
                    }
                }
            }
        }
    }
}

/// Handle a single WebSocket client connection.
async fn handle_ws_client(
    stream: TcpStream,
    peer: SocketAddr,
    mut rx: broadcast::Receiver<StreamFrame>,
) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!(peer = %peer, "WebSocket handshake failed: {e}");
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // Spawn a reader task to handle client pings/close frames.
    let reader_handle = tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Close(_)) => break,
                Ok(Message::Ping(data)) => {
                    // Pong is handled automatically by tungstenite in most cases.
                    let _ = data;
                }
                Err(e) => {
                    warn!(peer = %peer, "WebSocket read error: {e}");
                    break;
                }
                _ => {}
            }
        }
    });

    // Push frames to the client.
    loop {
        match rx.recv().await {
            Ok(frame) => {
                if let Err(e) = write.send(Message::Text(frame.json)).await {
                    warn!(peer = %peer, "WebSocket send error: {e}");
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(peer = %peer, lagged = n, "Client lagged behind — dropping frames");
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!(peer = %peer, "Broadcast channel closed — disconnecting client");
                break;
            }
        }
    }

    reader_handle.abort();
    info!(peer = %peer, "WebSocket client disconnected");
}

/// Push a new linearized snapshot to all connected WebSocket clients.
pub fn push_snapshot(
    tx: &broadcast::Sender<StreamFrame>,
    dag: &DagStore,
    coloring: &ColoringOutput,
) -> Result<usize, argus_ghostdag::GhostDagError> {
    let blocks = linearize_dag(dag, coloring)?;
    let json = serde_json::to_string(&blocks).map_err(|e| {
        argus_ghostdag::GhostDagError::Internal(format!("JSON serialization failed: {e}"))
    })?;
    let frame = StreamFrame { json };
    let receivers = tx.send(frame).unwrap_or(0);
    Ok(receivers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_ghostdag::block::{BlockHash, BlockHeader};
    use argus_ghostdag::coloring::color_dag;

    fn build_test_dag() -> (DagStore, ColoringOutput) {
        let mut dag = DagStore::new();
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);
        let b = BlockHash::from_byte(2);
        let c = BlockHash::from_byte(3);

        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(b, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(c, vec![a, b], 2)).unwrap();

        let coloring = color_dag(&mut dag, 3).unwrap();
        (dag, coloring)
    }

    #[test]
    fn test_linearize_dag() {
        let (dag, coloring) = build_test_dag();
        let blocks = linearize_dag(&dag, &coloring).unwrap();

        assert_eq!(blocks.len(), 4);
        // Each block must have a valid topological_index.
        for (i, b) in blocks.iter().enumerate() {
            assert_eq!(b.topological_index, i as u64);
        }
        // All should be blue with k=3 on a 4-block diamond.
        assert!(blocks.iter().all(|b| b.is_blue));
    }

    #[test]
    fn test_push_snapshot_no_receivers() {
        let (dag, coloring) = build_test_dag();
        let (tx, _) = broadcast::channel::<StreamFrame>(16);
        // No receivers subscribed — should still succeed.
        let count = push_snapshot(&tx, &dag, &coloring).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_linearized_block_has_adjacency() {
        let (dag, coloring) = build_test_dag();
        let blocks = linearize_dag(&dag, &coloring).unwrap();

        // Block C (from_byte(3)) has parents A and B.
        let c_block = blocks.iter().find(|b| {
            b.hash == BlockHash::from_byte(3).to_hex()
        }).unwrap();
        assert_eq!(c_block.adjacency_list.len(), 2);
    }
}
