/// Argus Agent — GhostDagAgent State Machine.
///
/// Implements the four-state autonomous agent:
///
/// - **SYNCED**: Local DAG matches network blue set.
/// - **DRIFTING**: Local selected-parent diverges from network.
/// - **RECOVERING**: Actively fetching missing anticone blocks.
/// - **PARTITIONED**: Divergence exceeds 3k threshold — network partition assumed.
///
/// Transitions are driven by `AgentCommand` messages received on an mpsc channel.
/// State changes emit `AgentEvent` messages.
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info, warn};

use argus_ghostdag::block::BlockHash;
use argus_ghostdag::coloring::{color_dag, selected_parent_chain};
use argus_ghostdag::dag::DagStore;

use crate::channels::{
    AgentCommand, AgentEvent, AgentStateLabel, CommandRx, EventTx,
};
use crate::lca::{greedy_path_intersection, has_diverged};

/// The GhostDagAgent — an autonomous state machine that monitors
/// DAG health and orchestrates recovery.
pub struct GhostDagAgent {
    /// Current agent state.
    state: AgentStateLabel,
    /// The shared DAG store (protected by RwLock for concurrent reads).
    dag: Arc<RwLock<DagStore>>,
    /// The local tip hash.
    local_tip: BlockHash,
    /// The most recent network tip we know about.
    network_tip: Option<BlockHash>,
    /// The GhostDAG k parameter (may be updated by RL optimizer).
    k: u64,
    /// Channel to receive commands from the orchestrator.
    cmd_rx: CommandRx,
    /// Channel to emit events to the orchestrator.
    event_tx: EventTx,
}

impl GhostDagAgent {
    /// Create a new agent.
    pub fn new(
        dag: Arc<RwLock<DagStore>>,
        local_tip: BlockHash,
        k: u64,
        cmd_rx: CommandRx,
        event_tx: EventTx,
    ) -> Self {
        Self {
            state: AgentStateLabel::Synced,
            dag,
            local_tip,
            network_tip: None,
            k,
            cmd_rx,
            event_tx,
        }
    }

    /// Returns the current state label.
    pub fn state(&self) -> AgentStateLabel {
        self.state
    }

    /// Run the agent event loop.  This consumes `self` and runs until
    /// a `Shutdown` command is received or the command channel closes.
    pub async fn run(mut self) {
        info!(state = %self.state, "GhostDagAgent started");

        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                AgentCommand::Shutdown => {
                    info!("GhostDagAgent shutting down");
                    break;
                }
                AgentCommand::UpdateNetworkTip { tip } => {
                    self.network_tip = Some(tip);
                    info!(?tip, "Network tip updated");
                }
                AgentCommand::UpdateK { new_k } => {
                    info!(old_k = self.k, new_k, "k parameter updated");
                    self.k = new_k;
                }
                AgentCommand::CheckDivergence { network_tip } => {
                    self.network_tip = Some(network_tip);
                    self.handle_divergence_check(network_tip).await;
                }
                AgentCommand::StartRecovery { lca, missing_blocks } => {
                    self.handle_start_recovery(lca, missing_blocks).await;
                }
            }
        }

        info!("GhostDagAgent event loop exited");
    }

    /// Transition to a new state, emitting a `StateChanged` event.
    async fn transition_to(&mut self, new_state: AgentStateLabel) {
        let old = self.state;
        if old == new_state {
            return;
        }
        self.state = new_state;
        info!(from = %old, to = %new_state, "State transition");

        if let Err(e) = self
            .event_tx
            .send(AgentEvent::StateChanged {
                from: old,
                to: new_state,
            })
            .await
        {
            error!("Failed to send state-change event: {e}");
        }
    }

    /// Handle a divergence check.
    async fn handle_divergence_check(&mut self, network_tip: BlockHash) {
        let dag = self.dag.read().await;

        // Build the network chain from our local knowledge.
        // In a full implementation this would come from a peer;
        // here we build it from the local DAG if the network tip
        // is present, or detect divergence otherwise.
        if !dag.contains(&network_tip) {
            // Network tip is not in our DAG — we're drifting.
            warn!(
                local_tip = %self.local_tip,
                network_tip = %network_tip,
                "Network tip not in local DAG — divergence detected"
            );
            drop(dag);
            self.transition_to(AgentStateLabel::Drifting).await;
            self.emit_divergence_detected(network_tip, 0).await;
            return;
        }

        // Build the network selected-parent chain.
        let network_chain = match selected_parent_chain(&dag, &network_tip) {
            Ok(chain) => chain,
            Err(e) => {
                error!("Failed to build network chain: {e}");
                return;
            }
        };

        // Check divergence.
        if has_diverged(&self.local_tip, &network_chain) {
            // Compute LCA.
            let lca_result = match greedy_path_intersection(&dag, &self.local_tip, &network_chain) {
                Ok(r) => r,
                Err(e) => {
                    error!("LCA computation failed: {e}");
                    return;
                }
            };

            // Check if divergence exceeds 3k threshold → PARTITIONED.
            if lca_result.divergence_depth > 3 * self.k {
                warn!(
                    depth = lca_result.divergence_depth,
                    threshold = 3 * self.k,
                    "Divergence exceeds 3k — assuming network partition"
                );
                drop(dag);
                self.transition_to(AgentStateLabel::Partitioned).await;
            } else {
                info!(
                    lca = %lca_result.lca,
                    missing = lca_result.missing_blocks.len(),
                    depth = lca_result.divergence_depth,
                    "Divergence detected — switching to DRIFTING"
                );
                drop(dag);
                self.transition_to(AgentStateLabel::Drifting).await;
                self.emit_divergence_detected(network_tip, lca_result.divergence_depth).await;
            }
        } else {
            // No divergence — ensure we're SYNCED.
            drop(dag);
            self.transition_to(AgentStateLabel::Synced).await;
        }
    }

    /// Handle the recovery process — fetch missing blocks and re-color.
    async fn handle_start_recovery(&mut self, _lca: BlockHash, missing_blocks: Vec<BlockHash>) {
        self.transition_to(AgentStateLabel::Recovering).await;

        let blocks_to_recover = missing_blocks.len() as u64;
        info!(
            blocks = blocks_to_recover,
            "Starting recovery — ingesting missing blocks"
        );

        // In a production system, we would fetch these blocks from peers.
        // Here we simulate: check if blocks are already present, and
        // re-run coloring.
        {
            let mut dag = self.dag.write().await;

            // Re-color the DAG with the current k.
            match color_dag(&mut dag, self.k) {
                Ok(coloring) => {
                    info!(
                        blue = coloring.blue_set.len(),
                        red = coloring.red_set.len(),
                        "DAG re-colored after recovery"
                    );
                }
                Err(e) => {
                    error!("Re-coloring failed during recovery: {e}");
                    let _ = self.event_tx.send(AgentEvent::Error {
                        message: format!("recovery re-coloring failed: {e}"),
                    }).await;
                    return;
                }
            }

            // Update local tip to the highest blue-score block.
            if let Some(new_tip) = dag
                .headers()
                .max_by_key(|h| h.blue_score)
                .map(|h| h.hash)
            {
                self.local_tip = new_tip;
            }
        }

        // Transition to SYNCED.
        self.transition_to(AgentStateLabel::Synced).await;

        if let Err(e) = self
            .event_tx
            .send(AgentEvent::RecoveryComplete {
                blocks_recovered: blocks_to_recover,
            })
            .await
        {
            error!("Failed to send recovery-complete event: {e}");
        }

        info!("Recovery complete — agent is SYNCED");
    }

    /// Emit a divergence-detected event.
    async fn emit_divergence_detected(&self, network_tip: BlockHash, depth: u64) {
        let _ = self
            .event_tx
            .send(AgentEvent::DivergenceDetected {
                local_tip: self.local_tip,
                network_tip,
                divergence_depth: depth,
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{command_channel, event_channel};
    use argus_ghostdag::block::BlockHeader;
    use argus_ghostdag::coloring::color_dag;

    async fn setup_synced_agent() -> (
        tokio::sync::mpsc::Sender<AgentCommand>,
        tokio::sync::mpsc::Receiver<AgentEvent>,
        tokio::task::JoinHandle<()>,
    ) {
        let mut dag = DagStore::new();
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);
        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        color_dag(&mut dag, 3).unwrap();

        let dag = Arc::new(RwLock::new(dag));
        let (cmd_tx, cmd_rx) = command_channel(32);
        let (event_tx, event_rx) = event_channel(32);

        let agent = GhostDagAgent::new(dag, a, 3, cmd_rx, event_tx);
        let handle = tokio::spawn(agent.run());

        (cmd_tx, event_rx, handle)
    }

    #[tokio::test]
    async fn test_synced_no_divergence() {
        let (cmd_tx, mut event_rx, handle) = setup_synced_agent().await;

        // Check divergence with the actual local tip — should stay synced.
        cmd_tx
            .send(AgentCommand::CheckDivergence {
                network_tip: BlockHash::from_byte(1),
            })
            .await
            .unwrap();

        // No state change event expected (already synced).
        // Shutdown and wait.
        cmd_tx.send(AgentCommand::Shutdown).await.unwrap();
        handle.await.unwrap();

        // Drain remaining events — should be empty or just one synced→synced (no-op).
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }
        // No divergence events.
        assert!(events.iter().all(|e| !matches!(e, AgentEvent::DivergenceDetected { .. })));
    }

    #[tokio::test]
    async fn test_divergence_detection() {
        let (cmd_tx, mut event_rx, handle) = setup_synced_agent().await;

        // Check divergence with a tip that's NOT in the local DAG.
        cmd_tx
            .send(AgentCommand::CheckDivergence {
                network_tip: BlockHash::from_byte(99),
            })
            .await
            .unwrap();

        // Wait briefly for processing.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should have received a StateChanged event to DRIFTING.
        let event = event_rx.recv().await.unwrap();
        match event {
            AgentEvent::StateChanged { from, to } => {
                assert_eq!(from, AgentStateLabel::Synced);
                assert_eq!(to, AgentStateLabel::Drifting);
            }
            other => panic!("Expected StateChanged, got {other:?}"),
        }

        cmd_tx.send(AgentCommand::Shutdown).await.unwrap();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_recovery_flow() {
        let (cmd_tx, mut event_rx, handle) = setup_synced_agent().await;

        // Trigger recovery directly.
        cmd_tx
            .send(AgentCommand::StartRecovery {
                lca: BlockHash::from_byte(0),
                missing_blocks: vec![BlockHash::from_byte(5), BlockHash::from_byte(6)],
            })
            .await
            .unwrap();

        // Should see: Synced→Recovering, then Recovering→Synced, then RecoveryComplete.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Verify we got a RecoveryComplete event.
        assert!(events.iter().any(|e| matches!(e, AgentEvent::RecoveryComplete { .. })));

        cmd_tx.send(AgentCommand::Shutdown).await.unwrap();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_update_k() {
        let (cmd_tx, _event_rx, handle) = setup_synced_agent().await;

        // Send a k update — should not error.
        cmd_tx
            .send(AgentCommand::UpdateK { new_k: 10 })
            .await
            .unwrap();

        cmd_tx.send(AgentCommand::Shutdown).await.unwrap();
        handle.await.unwrap();
    }
}
