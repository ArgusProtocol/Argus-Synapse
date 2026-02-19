/// Argus Agent — Async Recovery Loop.
///
/// Provides a background task that continuously monitors the DAG for
/// divergence and triggers automatic recovery when needed.
///
/// The recovery loop:
/// 1. Periodically checks local tip vs. network tip.
/// 2. If divergence is detected, computes the LCA.
/// 3. Fetches missing blocks within the anticone(B) threshold k.
/// 4. Re-colors the DAG and transitions back to SYNCED.
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use argus_ghostdag::block::{BlockHash, BlockHeader};
use argus_ghostdag::coloring::{color_dag, selected_parent_chain};
use argus_ghostdag::dag::DagStore;

use crate::channels::{AgentCommand, AgentEvent, AgentStateLabel, CommandTx, EventTx};
use crate::lca::{greedy_path_intersection, has_diverged, LcaResult};

/// Configuration for the recovery loop.
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// How often to check for divergence (in milliseconds).
    pub check_interval_ms: u64,
    /// The k parameter for anticone threshold.
    pub k: u64,
    /// Maximum blocks to recover in a single pass.
    pub max_recovery_batch: usize,
    /// Divergence depth multiplier for partition detection (default: 3).
    pub partition_threshold_multiplier: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            check_interval_ms: 1000,
            k: 3,
            max_recovery_batch: 256,
            partition_threshold_multiplier: 3,
        }
    }
}

/// A standalone recovery loop that runs alongside the `GhostDagAgent`.
///
/// This component monitors the DAG and sends commands to the agent
/// when action is needed.
pub struct RecoveryLoop {
    dag: Arc<RwLock<DagStore>>,
    config: RecoveryConfig,
    cmd_tx: CommandTx,
    event_tx: EventTx,
    /// The latest known network tip (updated externally).
    network_tip: Arc<RwLock<Option<BlockHash>>>,
    /// Shutdown signal.
    shutdown: tokio::sync::watch::Receiver<bool>,
}

impl RecoveryLoop {
    /// Create a new recovery loop.
    pub fn new(
        dag: Arc<RwLock<DagStore>>,
        config: RecoveryConfig,
        cmd_tx: CommandTx,
        event_tx: EventTx,
        network_tip: Arc<RwLock<Option<BlockHash>>>,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        Self {
            dag,
            config,
            cmd_tx,
            event_tx,
            network_tip,
            shutdown,
        }
    }

    /// Run the recovery monitoring loop until shutdown is signaled.
    pub async fn run(mut self) {
        info!(
            interval_ms = self.config.check_interval_ms,
            k = self.config.k,
            "Recovery loop started"
        );

        let interval = Duration::from_millis(self.config.check_interval_ms);

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    self.tick().await;
                }
                Ok(()) = self.shutdown.changed() => {
                    if *self.shutdown.borrow() {
                        info!("Recovery loop shutting down");
                        break;
                    }
                }
            }
        }
    }

    /// A single tick of the monitoring loop.
    async fn tick(&self) {
        let network_tip = {
            let guard = self.network_tip.read().await;
            match *guard {
                Some(tip) => tip,
                None => {
                    debug!("No network tip known yet — skipping divergence check");
                    return;
                }
            }
        };

        let dag = self.dag.read().await;

        // Find our local tip (highest blue-score block).
        let local_tip = match dag.headers().max_by_key(|h| h.blue_score).map(|h| h.hash) {
            Some(tip) => tip,
            None => {
                debug!("DAG is empty — skipping divergence check");
                return;
            }
        };

        // Quick check: is the network tip in our DAG?
        if !dag.contains(&network_tip) {
            warn!(
                local = %local_tip,
                network = %network_tip,
                "Network tip not in local DAG — sending divergence check"
            );
            drop(dag);
            let _ = self
                .cmd_tx
                .send(AgentCommand::CheckDivergence { network_tip })
                .await;
            return;
        }

        // Build network chain and check divergence.
        let network_chain = match selected_parent_chain(&dag, &network_tip) {
            Ok(chain) => chain,
            Err(e) => {
                error!("Failed to build network selected-parent chain: {e}");
                return;
            }
        };

        if !has_diverged(&local_tip, &network_chain) {
            debug!("No divergence — local tip is on network chain");
            return;
        }

        // Compute LCA.
        let lca_result: LcaResult =
            match greedy_path_intersection(&dag, &local_tip, &network_chain) {
                Ok(r) => r,
                Err(e) => {
                    error!("LCA computation failed: {e}");
                    return;
                }
            };

        let partition_threshold = self.config.partition_threshold_multiplier * self.config.k;

        if lca_result.divergence_depth > partition_threshold {
            warn!(
                depth = lca_result.divergence_depth,
                threshold = partition_threshold,
                "Divergence exceeds partition threshold"
            );
            let _ = self
                .event_tx
                .send(AgentEvent::DivergenceDetected {
                    local_tip,
                    network_tip,
                    divergence_depth: lca_result.divergence_depth,
                })
                .await;
            return;
        }

        // Trigger recovery.
        info!(
            lca = %lca_result.lca,
            missing = lca_result.missing_blocks.len(),
            "Divergence detected — triggering recovery"
        );

        let missing = if lca_result.missing_blocks.len() > self.config.max_recovery_batch {
            lca_result.missing_blocks[..self.config.max_recovery_batch].to_vec()
        } else {
            lca_result.missing_blocks
        };

        drop(dag);

        let _ = self
            .cmd_tx
            .send(AgentCommand::StartRecovery {
                lca: lca_result.lca,
                missing_blocks: missing,
            })
            .await;
    }
}

/// Ingest a batch of new blocks into the DAG, re-color, and return
/// the updated coloring.
///
/// This is used by the recovery path to merge missing blocks.
pub async fn ingest_and_recolor(
    dag: &Arc<RwLock<DagStore>>,
    blocks: Vec<BlockHeader>,
    k: u64,
) -> Result<argus_ghostdag::ColoringOutput, argus_ghostdag::GhostDagError> {
    let mut dag = dag.write().await;

    for block in blocks {
        // Skip blocks already present (idempotent ingestion).
        if dag.contains(&block.hash) {
            continue;
        }
        // Only add if all parents are present.
        let parents_present = block.parents.iter().all(|p| dag.contains(p));
        if parents_present {
            dag.add_block(block)?;
        } else {
            warn!("Skipping block with missing parents during recovery");
        }
    }

    color_dag(&mut dag, k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{command_channel, event_channel};
    use argus_ghostdag::block::{BlockHash, BlockHeader};
    use argus_ghostdag::coloring::color_dag;

    #[tokio::test]
    async fn test_ingest_and_recolor() {
        let mut dag = DagStore::new();
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);
        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        color_dag(&mut dag, 3).unwrap();

        let dag = Arc::new(RwLock::new(dag));

        // Ingest a new block.
        let b = BlockHash::from_byte(2);
        let new_block = BlockHeader::new(b, vec![a], 2);
        let coloring = ingest_and_recolor(&dag, vec![new_block], 3).await.unwrap();

        // All 3 blocks should be blue with k=3.
        assert_eq!(coloring.blue_set.len(), 3);
    }

    #[tokio::test]
    async fn test_recovery_loop_no_crash() {
        let mut dag = DagStore::new();
        let g = BlockHash::from_byte(0);
        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        color_dag(&mut dag, 3).unwrap();

        let dag = Arc::new(RwLock::new(dag));
        let (cmd_tx, _cmd_rx) = command_channel(32);
        let (event_tx, _event_rx) = event_channel(32);
        let network_tip = Arc::new(RwLock::new(None));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let config = RecoveryConfig {
            check_interval_ms: 50,
            ..Default::default()
        };

        let recovery = RecoveryLoop::new(
            dag,
            config,
            cmd_tx,
            event_tx,
            network_tip,
            shutdown_rx,
        );

        let handle = tokio::spawn(recovery.run());

        // Let it tick a few times.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Signal shutdown.
        shutdown_tx.send(true).unwrap();
        handle.await.unwrap();
    }
}
