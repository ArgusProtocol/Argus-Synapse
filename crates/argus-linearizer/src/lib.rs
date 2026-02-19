/// Argus Linearizer — Linearization Engine for GNNs.
///
/// This crate provides:
///
/// - **`schema`** — GNN-ready JSON schemas (`LinearizedBlock`, `DagSnapshot`, etc.).
/// - **`stream`** — WebSocket producer for broadcasting linearized DAG frames.
/// - **`server`** — JSON-RPC + WebSocket server with hot-swap k, smart submit, etc.

pub mod schema;
pub mod server;
pub mod stream;

// Re-exports.
pub use schema::{
    AgentHealth, DagSnapshot, LinearizedBlock, SmartSubmitRequest, SmartSubmitResponse,
};
pub use server::{start_server, ServerConfig, ServerState};
pub use stream::{linearize_dag, push_snapshot, StreamFrame, WsProducer};
