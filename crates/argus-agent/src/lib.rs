/// Argus Agent — Autonomous Self-Healing for GhostDAG nodes.
///
/// This crate provides:
///
/// - **`state_machine`** — The `GhostDagAgent` state machine (SYNCED / DRIFTING / RECOVERING / PARTITIONED).
/// - **`lca`** — Greedy Path Intersection for computing the Lowest Common Ancestor.
/// - **`recovery`** — Async recovery loops for automatic DAG repair.
/// - **`channels`** — Typed mpsc channel definitions for agent communication.

pub mod channels;
pub mod lca;
pub mod recovery;
pub mod state_machine;

// Re-exports.
pub use channels::{
    command_channel, event_channel, AgentCommand, AgentEvent, AgentStateLabel,
    CommandRx, CommandTx, EventRx, EventTx,
};
pub use lca::{greedy_path_intersection, has_diverged, LcaResult};
pub use recovery::{ingest_and_recolor, RecoveryConfig, RecoveryLoop};
pub use state_machine::GhostDagAgent;
