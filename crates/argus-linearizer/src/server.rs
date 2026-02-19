/// Argus Linearizer — JSON-RPC + WebSocket server.
///
/// Exposes two endpoints:
/// - **WebSocket** on port 9292: streams linearized blocks.
/// - **JSON-RPC** on port 9293: request/response queries over the DAG.
///
/// JSON-RPC methods:
/// - `linearize_range(from_score, to_score)` — returns blocks in a blue-score range.
/// - `get_tip_order()` — returns the full PHANTOM total ordering.
/// - `get_tips()` — returns current DAG tips with blue scores.
/// - `get_snapshot(n)` — returns the last N blocks (GNN sub-graph).
/// - `get_health()` — returns agent health info.
/// - `update_k(new_k)` — hot-swaps the k parameter.
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};

use argus_ghostdag::block::BlockHash;
use argus_ghostdag::coloring::{color_dag, ColoringOutput};
use argus_ghostdag::dag::DagStore;
use argus_ghostdag::ordering::total_order;

use crate::schema::{
    AgentHealth, DagSnapshot, LinearizedBlock, SmartSubmitRequest, SmartSubmitResponse,
};
use crate::stream::{linearize_dag, push_snapshot, StreamFrame, WsProducer};

/// Shared state for the JSON-RPC + WebSocket server.
pub struct ServerState {
    /// The DAG store (shared with the agent).
    pub dag: Arc<RwLock<DagStore>>,
    /// Current coloring result.
    pub coloring: Arc<RwLock<Option<ColoringOutput>>>,
    /// Current k parameter.
    pub k: Arc<RwLock<u64>>,
    /// Agent state label (string).
    pub agent_state: Arc<RwLock<String>>,
    /// RL confidence score.
    pub rl_confidence: Arc<RwLock<f64>>,
    /// WebSocket broadcast sender.
    pub ws_tx: broadcast::Sender<StreamFrame>,
}

impl ServerState {
    /// Re-color the DAG and push a new snapshot to WebSocket clients.
    pub async fn recolor_and_broadcast(&self) -> Result<(), argus_ghostdag::GhostDagError> {
        let k = *self.k.read().await;
        let coloring = {
            let mut dag = self.dag.write().await;
            let c = color_dag(&mut dag, k)?;
            c
        };

        // Push snapshot.
        {
            let dag = self.dag.read().await;
            let _ = push_snapshot(&self.ws_tx, &dag, &coloring);
        }

        // Store coloring.
        *self.coloring.write().await = Some(coloring);
        Ok(())
    }

    /// Get all tips sorted by blue score descending.
    pub async fn get_bluest_tips(&self, count: usize) -> Vec<(BlockHash, u64)> {
        let dag = self.dag.read().await;
        let mut tips: Vec<(BlockHash, u64)> = dag
            .tips()
            .into_iter()
            .filter_map(|h| dag.get(&h).ok().map(|hdr| (h, hdr.blue_score)))
            .collect();
        tips.sort_by(|a, b| b.1.cmp(&a.1));
        tips.truncate(count);
        tips
    }

    /// Get a snapshot of the last N blocks.
    pub async fn get_snapshot(&self, n: usize) -> Result<DagSnapshot, argus_ghostdag::GhostDagError> {
        let dag = self.dag.read().await;
        let coloring_guard = self.coloring.read().await;
        let coloring = coloring_guard
            .as_ref()
            .ok_or_else(|| argus_ghostdag::GhostDagError::Internal("DAG not colored yet".into()))?;

        let all_blocks = linearize_dag(&dag, coloring)?;
        let total = all_blocks.len();
        let start = if total > n { total - n } else { 0 };
        let blocks = all_blocks[start..].to_vec();

        let tip = dag
            .tips()
            .into_iter()
            .filter_map(|h| dag.get(&h).ok().map(|hdr| (h, hdr.blue_score)))
            .max_by_key(|(_, score)| *score)
            .map(|(h, _)| h.to_hex())
            .unwrap_or_default();

        let k = *self.k.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(DagSnapshot {
            blocks,
            total_blocks: total as u64,
            k,
            tip,
            generated_at: now,
        })
    }

    /// Get agent health.
    pub async fn get_health(&self) -> AgentHealth {
        let dag = self.dag.read().await;
        let coloring_guard = self.coloring.read().await;
        let k = *self.k.read().await;
        let confidence = *self.rl_confidence.read().await;
        let state = self.agent_state.read().await.clone();

        let (blue_count, red_count) = match coloring_guard.as_ref() {
            Some(c) => (c.blue_set.len() as u64, c.red_set.len() as u64),
            None => (0, 0),
        };

        let tip_blue_score = dag
            .headers()
            .map(|h| h.blue_score)
            .max()
            .unwrap_or(0);

        AgentHealth {
            current_k: k,
            rl_confidence: confidence,
            agent_state: state,
            tip_blue_score,
            total_blocks: dag.len() as u64,
            blue_count,
            red_count,
        }
    }

    /// Handle smart submit — select the 3-5 bluest tips as parents.
    pub async fn smart_submit(&self, req: SmartSubmitRequest) -> SmartSubmitResponse {
        let count = req.parent_count.clamp(3, 5);
        let tips = self.get_bluest_tips(count).await;

        let selected_parents: Vec<String> = tips.iter().map(|(h, _)| h.to_hex()).collect();
        let parent_blue_scores: Vec<u64> = tips.iter().map(|(_, s)| *s).collect();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        SmartSubmitResponse {
            accepted: !selected_parents.is_empty(),
            selected_parents,
            parent_blue_scores,
            suggested_timestamp: now,
        }
    }

    /// Get blocks in a blue-score range.
    pub async fn linearize_range(
        &self,
        from_score: u64,
        to_score: u64,
    ) -> Result<Vec<LinearizedBlock>, argus_ghostdag::GhostDagError> {
        let dag = self.dag.read().await;
        let coloring_guard = self.coloring.read().await;
        let coloring = coloring_guard
            .as_ref()
            .ok_or_else(|| argus_ghostdag::GhostDagError::Internal("DAG not colored yet".into()))?;

        let all = linearize_dag(&dag, coloring)?;
        let filtered: Vec<LinearizedBlock> = all
            .into_iter()
            .filter(|b| b.blue_score >= from_score && b.blue_score <= to_score)
            .collect();
        Ok(filtered)
    }

    /// Hot-swap the k parameter and re-color.
    pub async fn update_k(&self, new_k: u64) -> Result<(), argus_ghostdag::GhostDagError> {
        if new_k < 1 {
            return Err(argus_ghostdag::GhostDagError::InvalidK(new_k));
        }
        *self.k.write().await = new_k;
        self.recolor_and_broadcast().await
    }
}

/// Configuration for the server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub ws_addr: SocketAddr,
    pub rpc_addr: SocketAddr,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            ws_addr: "0.0.0.0:9292".parse().unwrap(),
            rpc_addr: "0.0.0.0:9293".parse().unwrap(),
        }
    }
}

/// Start the complete server (WebSocket + JSON-RPC).
///
/// This is the main entry point for running the linearizer
/// as a standalone service.
pub async fn start_server(
    state: Arc<ServerState>,
    config: ServerConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) {
    info!(
        ws = %config.ws_addr,
        rpc = %config.rpc_addr,
        "Starting Argus Linearizer server"
    );

    // Start WebSocket producer.
    let (ws_producer, _ws_tx) = WsProducer::new(config.ws_addr);
    let ws_shutdown = shutdown.clone();
    let ws_handle = tokio::spawn(async move {
        ws_producer.run(ws_shutdown).await;
    });

    // Start a simple JSON-RPC loop using TCP.
    // In production, this would use jsonrpsee; here we implement
    // a lightweight request/response handler.
    let rpc_shutdown = shutdown.clone();
    let rpc_state = state.clone();
    let rpc_handle = tokio::spawn(async move {
        run_rpc_server(rpc_state, config.rpc_addr, rpc_shutdown).await;
    });

    // Wait for both to complete.
    let _ = tokio::join!(ws_handle, rpc_handle);
    info!("Argus Linearizer server stopped");
}

/// A minimal JSON-RPC server over TCP.
async fn run_rpc_server(
    state: Arc<ServerState>,
    addr: SocketAddr,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => {
            info!(addr = %addr, "JSON-RPC server listening");
            l
        }
        Err(e) => {
            error!(addr = %addr, "Failed to bind RPC listener: {e}");
            return;
        }
    };

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((mut stream, peer)) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 4096];
                            match stream.read(&mut buf).await {
                                Ok(n) if n > 0 => {
                                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                                    let response = handle_rpc_request(&state, &request).await;
                                    let _ = stream.write_all(response.as_bytes()).await;
                                }
                                _ => {}
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept RPC connection: {e}");
                    }
                }
            }
            Ok(()) = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("JSON-RPC server shutting down");
                    break;
                }
            }
        }
    }
}

/// Handle a single JSON-RPC request string and return a response string.
async fn handle_rpc_request(state: &Arc<ServerState>, request: &str) -> String {
    // Parse minimal JSON-RPC 2.0.
    #[derive(serde::Deserialize)]
    struct RpcRequest {
        #[allow(dead_code)]
        jsonrpc: Option<String>,
        method: String,
        params: Option<serde_json::Value>,
        id: Option<serde_json::Value>,
    }

    #[derive(serde::Serialize)]
    struct RpcResponse {
        jsonrpc: &'static str,
        result: Option<serde_json::Value>,
        error: Option<serde_json::Value>,
        id: serde_json::Value,
    }

    let parsed: RpcRequest = match serde_json::from_str(request) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::to_string(&serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32700, "message": format!("Parse error: {e}")},
                "id": null
            }))
            .unwrap_or_default();
        }
    };

    let id = parsed.id.unwrap_or(serde_json::Value::Null);
    let params = parsed.params.unwrap_or(serde_json::Value::Null);

    let result: Result<serde_json::Value, String> = match parsed.method.as_str() {
        "get_tip_order" => {
            let dag = state.dag.read().await;
            let coloring_guard = state.coloring.read().await;
            match coloring_guard.as_ref() {
                Some(coloring) => match linearize_dag(&dag, coloring) {
                    Ok(blocks) => Ok(serde_json::to_value(blocks).unwrap()),
                    Err(e) => Err(e.to_string()),
                },
                None => Err("DAG not colored yet".to_string()),
            }
        }
        "get_tips" => {
            let tips = state.get_bluest_tips(10).await;
            let tip_data: Vec<serde_json::Value> = tips
                .into_iter()
                .map(|(h, s)| serde_json::json!({"hash": h.to_hex(), "blue_score": s}))
                .collect();
            Ok(serde_json::Value::Array(tip_data))
        }
        "get_snapshot" => {
            let n = params
                .get("n")
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as usize;
            match state.get_snapshot(n).await {
                Ok(snapshot) => Ok(serde_json::to_value(snapshot).unwrap()),
                Err(e) => Err(e.to_string()),
            }
        }
        "get_health" => {
            let health = state.get_health().await;
            Ok(serde_json::to_value(health).unwrap())
        }
        "linearize_range" => {
            let from = params.get("from_score").and_then(|v| v.as_u64()).unwrap_or(0);
            let to = params.get("to_score").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
            match state.linearize_range(from, to).await {
                Ok(blocks) => Ok(serde_json::to_value(blocks).unwrap()),
                Err(e) => Err(e.to_string()),
            }
        }
        "update_k" => {
            let new_k = params
                .get("new_k")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "missing 'new_k' parameter".to_string())?;
            match state.update_k(new_k).await {
                Ok(()) => Ok(serde_json::json!({"updated_k": new_k})),
                Err(e) => Err(e.to_string()),
            }
        }
        "smart_submit" => {
            let req: SmartSubmitRequest = serde_json::from_value(params)
                .map_err(|e| format!("invalid request: {e}"))?;
            let resp = state.smart_submit(req).await;
            Ok(serde_json::to_value(resp).unwrap())
        }
        other => Err(format!("unknown method: {other}")),
    };

    let resp = match result {
        Ok(value) => RpcResponse {
            jsonrpc: "2.0",
            result: Some(value),
            error: None,
            id,
        },
        Err(msg) => RpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(serde_json::json!({"code": -32603, "message": msg})),
            id,
        },
    };

    serde_json::to_string(&resp).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_ghostdag::block::{BlockHash, BlockHeader};
    use argus_ghostdag::coloring::color_dag;

    async fn build_test_state() -> Arc<ServerState> {
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
        let (ws_tx, _) = broadcast::channel(16);

        Arc::new(ServerState {
            dag: Arc::new(RwLock::new(dag)),
            coloring: Arc::new(RwLock::new(Some(coloring))),
            k: Arc::new(RwLock::new(3)),
            agent_state: Arc::new(RwLock::new("SYNCED".to_string())),
            rl_confidence: Arc::new(RwLock::new(0.95)),
            ws_tx,
        })
    }

    #[tokio::test]
    async fn test_get_snapshot() {
        let state = build_test_state().await;
        let snapshot = state.get_snapshot(10).await.unwrap();
        assert_eq!(snapshot.blocks.len(), 4);
        assert_eq!(snapshot.k, 3);
    }

    #[tokio::test]
    async fn test_get_health() {
        let state = build_test_state().await;
        let health = state.get_health().await;
        assert_eq!(health.current_k, 3);
        assert_eq!(health.total_blocks, 4);
        assert!(health.rl_confidence > 0.0);
    }

    #[tokio::test]
    async fn test_smart_submit() {
        let state = build_test_state().await;
        let req = SmartSubmitRequest {
            payload: "deadbeef".to_string(),
            parent_count: 3,
        };
        let resp = state.smart_submit(req).await;
        assert!(resp.accepted);
        // Only 1 tip (C) in a diamond DAG.
        assert!(!resp.selected_parents.is_empty());
    }

    #[tokio::test]
    async fn test_linearize_range() {
        let state = build_test_state().await;
        let blocks = state.linearize_range(0, 100).await.unwrap();
        assert_eq!(blocks.len(), 4);
    }

    #[tokio::test]
    async fn test_update_k() {
        let state = build_test_state().await;
        state.update_k(5).await.unwrap();
        assert_eq!(*state.k.read().await, 5);
    }

    #[tokio::test]
    async fn test_rpc_handler() {
        let state = build_test_state().await;
        let request = r#"{"jsonrpc":"2.0","method":"get_health","id":1}"#;
        let response = handle_rpc_request(&state, request).await;
        assert!(response.contains("current_k"));
    }

    #[tokio::test]
    async fn test_rpc_unknown_method() {
        let state = build_test_state().await;
        let request = r#"{"jsonrpc":"2.0","method":"foo","id":1}"#;
        let response = handle_rpc_request(&state, request).await;
        assert!(response.contains("unknown method"));
    }
}
