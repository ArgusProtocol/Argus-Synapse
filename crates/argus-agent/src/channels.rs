/// Argus Agent — Channel types for async communication.
///
/// Defines the command and event enums sent between the orchestrator
/// and the `GhostDagAgent` via `tokio::sync::mpsc` channels.
use argus_ghostdag::BlockHash;
use serde::{Deserialize, Serialize};

/// Commands sent *to* the GhostDagAgent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentCommand {
    /// Check whether the local selected-parent chain diverges
    /// from the network's blue set.
    CheckDivergence {
        /// The network's current tip (highest blue-score block).
        network_tip: BlockHash,
    },

    /// Begin recovery from the given Lowest Common Ancestor.
    StartRecovery {
        /// The LCA hash computed by the Greedy Path Intersection.
        lca: BlockHash,
        /// Block hashes that exist on the network chain but are missing locally.
        missing_blocks: Vec<BlockHash>,
    },

    /// Inform the agent of an updated network tip.
    UpdateNetworkTip {
        tip: BlockHash,
    },

    /// Update the k parameter (from the RL optimizer).
    UpdateK {
        new_k: u64,
    },

    /// Request a graceful shutdown.
    Shutdown,
}

/// Events emitted *by* the GhostDagAgent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    /// The agent transitioned between states.
    StateChanged {
        from: AgentStateLabel,
        to: AgentStateLabel,
    },

    /// Recovery completed — the agent is back in sync.
    RecoveryComplete {
        blocks_recovered: u64,
    },

    /// The agent detected divergence.
    DivergenceDetected {
        local_tip: BlockHash,
        network_tip: BlockHash,
        divergence_depth: u64,
    },

    /// An error occurred inside the agent.
    Error {
        message: String,
    },
}

/// Labels for serializable agent states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStateLabel {
    Synced,
    Drifting,
    Recovering,
    Partitioned,
}

impl std::fmt::Display for AgentStateLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synced => write!(f, "SYNCED"),
            Self::Drifting => write!(f, "DRIFTING"),
            Self::Recovering => write!(f, "RECOVERING"),
            Self::Partitioned => write!(f, "PARTITIONED"),
        }
    }
}

/// Typed sender/receiver wrappers for ergonomic channel usage.
pub type CommandTx = tokio::sync::mpsc::Sender<AgentCommand>;
pub type CommandRx = tokio::sync::mpsc::Receiver<AgentCommand>;
pub type EventTx = tokio::sync::mpsc::Sender<AgentEvent>;
pub type EventRx = tokio::sync::mpsc::Receiver<AgentEvent>;

/// Create a pair of command channels with the given buffer size.
pub fn command_channel(buffer: usize) -> (CommandTx, CommandRx) {
    tokio::sync::mpsc::channel(buffer)
}

/// Create a pair of event channels with the given buffer size.
pub fn event_channel(buffer: usize) -> (EventTx, EventRx) {
    tokio::sync::mpsc::channel(buffer)
}
