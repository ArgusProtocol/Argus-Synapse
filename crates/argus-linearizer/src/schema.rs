/// Argus Linearizer — GNN-ready JSON schema.
///
/// Defines the wire format for linearized blocks streamed to GNN consumers
/// via WebSocket and returned by JSON-RPC.
use serde::{Deserialize, Serialize};

/// A single linearized block in the GNN-ready format.
///
/// This is the canonical schema sent over the WebSocket stream and
/// returned by all API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearizedBlock {
    /// Block hash (hex-encoded).
    pub hash: String,

    /// Blue score — number of blue blocks in `past(B)`.
    pub blue_score: u64,

    /// Blue work — cumulative PoW from blue ancestors (decimal string).
    pub blue_work: String,

    /// Position in the PHANTOM total ordering.
    pub topological_index: u64,

    /// Adjacency list — parent hashes (hex-encoded).
    pub adjacency_list: Vec<String>,

    /// Whether this block was colored blue by the k-cluster algorithm.
    pub is_blue: bool,

    /// Selected parent hash (hex-encoded), if any.
    pub selected_parent: Option<String>,
}

/// A GNN-ready sub-graph snapshot containing multiple linearized blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagSnapshot {
    /// The linearized blocks in total-order sequence.
    pub blocks: Vec<LinearizedBlock>,

    /// Total number of blocks in the snapshot.
    pub total_blocks: u64,

    /// The k parameter used for coloring.
    pub k: u64,

    /// Tip hash (hex-encoded).
    pub tip: String,

    /// Timestamp when this snapshot was generated (ms since epoch).
    pub generated_at: u64,
}

/// Health status for the /agent/health endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    /// Current GhostDAG k parameter.
    pub current_k: u64,

    /// The RL agent's confidence score (0.0 to 1.0).
    pub rl_confidence: f64,

    /// Current agent state as a string.
    pub agent_state: String,

    /// Latest blue score at the tip.
    pub tip_blue_score: u64,

    /// Total blocks in the DAG.
    pub total_blocks: u64,

    /// Number of blue blocks.
    pub blue_count: u64,

    /// Number of red blocks.
    pub red_count: u64,
}

/// Response for the POST /tx/submit-smart endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartSubmitResponse {
    /// Whether the submission was accepted.
    pub accepted: bool,

    /// The selected parent hashes (hex-encoded) — the 3-5 "bluest" tips.
    pub selected_parents: Vec<String>,

    /// Blue scores of the selected parents.
    pub parent_blue_scores: Vec<u64>,

    /// Suggested timestamp for the new block.
    pub suggested_timestamp: u64,
}

/// Request body for POST /tx/submit-smart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartSubmitRequest {
    /// The transaction payload (opaque bytes, hex-encoded).
    pub payload: String,

    /// Optional: how many parents to select (default: 3, max: 5).
    #[serde(default = "default_parent_count")]
    pub parent_count: usize,
}

fn default_parent_count() -> usize {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linearized_block_serde() {
        let block = LinearizedBlock {
            hash: "abc123".to_string(),
            blue_score: 42,
            blue_work: "1000".to_string(),
            topological_index: 7,
            adjacency_list: vec!["parent1".to_string()],
            is_blue: true,
            selected_parent: Some("parent1".to_string()),
        };

        let json = serde_json::to_string(&block).unwrap();
        let decoded: LinearizedBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.hash, "abc123");
        assert_eq!(decoded.blue_score, 42);
        assert!(decoded.is_blue);
    }

    #[test]
    fn test_smart_submit_request_defaults() {
        let json = r#"{"payload": "deadbeef"}"#;
        let req: SmartSubmitRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.parent_count, 3);
    }

    #[test]
    fn test_dag_snapshot_serde() {
        let snapshot = DagSnapshot {
            blocks: vec![],
            total_blocks: 0,
            k: 3,
            tip: "aaa".to_string(),
            generated_at: 1234567890,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: DagSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.k, 3);
    }
}
