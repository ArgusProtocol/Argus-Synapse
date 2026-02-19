/// Argus Agent — Greedy Path Intersection / Lowest Common Ancestor.
///
/// Given a local DAG and a "network" selected-parent chain, finds the
/// first block where the two chains intersect — the Lowest Common Ancestor.
///
/// Algorithm:
/// 1. Walk the selected-parent chain from `local_tip` and `network_tip`
///    simultaneously.
/// 2. Maintain visited sets for each side.
/// 3. The first hash found in both visited sets is the LCA.
/// 4. All blocks on the network chain between the LCA and `network_tip`
///    that are NOT in the local DAG are the "missing blocks" to recover.
use std::collections::HashSet;

use argus_ghostdag::block::BlockHash;
use argus_ghostdag::dag::DagStore;
use argus_ghostdag::errors::{GhostDagError, GhostDagResult};

/// Result of the Greedy Path Intersection.
#[derive(Debug, Clone)]
pub struct LcaResult {
    /// The Lowest Common Ancestor hash.
    pub lca: BlockHash,
    /// Blocks on the network chain that are missing from the local DAG,
    /// ordered from LCA (exclusive) toward the network tip (inclusive).
    pub missing_blocks: Vec<BlockHash>,
    /// Depth of divergence (number of blocks between LCA and local tip).
    pub divergence_depth: u64,
}

/// Compute the LCA between a local DAG and a network selected-parent chain.
///
/// # Arguments
///
/// * `local_dag` — The local DAG store (may be incomplete).
/// * `local_tip` — The local node's current tip.
/// * `network_chain` — The network's selected-parent chain from genesis to
///   network tip (inclusive), ordered genesis-first.
///
/// # Returns
///
/// `LcaResult` containing the LCA, missing blocks, and divergence depth.
pub fn greedy_path_intersection(
    local_dag: &DagStore,
    local_tip: &BlockHash,
    network_chain: &[BlockHash],
) -> GhostDagResult<LcaResult> {
    if network_chain.is_empty() {
        return Err(GhostDagError::Internal(
            "network chain is empty".to_string(),
        ));
    }

    // Build the local selected-parent chain.
    let local_chain = build_selected_parent_chain(local_dag, local_tip)?;
    let local_set: HashSet<BlockHash> = local_chain.iter().copied().collect();

    // Walk the network chain from tip backward to find the LCA.
    let mut lca: Option<BlockHash> = None;
    let mut lca_network_idx: Option<usize> = None;

    for (idx, hash) in network_chain.iter().enumerate().rev() {
        if local_set.contains(hash) {
            lca = Some(*hash);
            lca_network_idx = Some(idx);
            break;
        }
    }

    let lca = lca.ok_or_else(|| {
        GhostDagError::Internal("no common ancestor found between local and network chains".to_string())
    })?;
    let lca_network_idx = lca_network_idx.unwrap();

    // Missing blocks: everything on network chain after the LCA
    // that is NOT in the local DAG.
    let missing_blocks: Vec<BlockHash> = network_chain[lca_network_idx + 1..]
        .iter()
        .filter(|h| !local_dag.contains(h))
        .copied()
        .collect();

    // Divergence depth: distance from LCA to local_tip on the local chain.
    let divergence_depth = local_chain
        .iter()
        .position(|h| *h == lca)
        .map(|pos| (local_chain.len() - 1 - pos) as u64)
        .unwrap_or(0);

    Ok(LcaResult {
        lca,
        missing_blocks,
        divergence_depth,
    })
}

/// Build the selected-parent chain from `tip` back to genesis.
/// Returns the chain ordered genesis-first.
fn build_selected_parent_chain(
    dag: &DagStore,
    tip: &BlockHash,
) -> GhostDagResult<Vec<BlockHash>> {
    let mut chain = vec![*tip];
    let mut current = *tip;

    loop {
        let hdr = dag.get(&current)?;
        match hdr.selected_parent {
            Some(sp) => {
                chain.push(sp);
                current = sp;
            }
            None => break, // reached genesis
        }
    }

    chain.reverse();
    Ok(chain)
}

/// Check whether the local tip diverges from the network chain.
/// Returns `true` if the local tip is NOT on the network chain.
pub fn has_diverged(
    local_tip: &BlockHash,
    network_chain: &[BlockHash],
) -> bool {
    !network_chain.contains(local_tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_ghostdag::block::BlockHeader;
    use argus_ghostdag::coloring::color_dag;

    /// Build two DAGs that share genesis + A but diverge at B/C:
    ///
    /// Local:   G → A → B
    /// Network: G → A → C → D
    fn build_divergent_dags() -> (DagStore, Vec<BlockHash>) {
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);
        let b = BlockHash::from_byte(2);
        let c = BlockHash::from_byte(3);
        let d = BlockHash::from_byte(4);

        // Local DAG: G → A → B
        let mut local_dag = DagStore::new();
        local_dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        local_dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        local_dag.add_block(BlockHeader::new(b, vec![a], 2)).unwrap();
        color_dag(&mut local_dag, 3).unwrap();

        // Network chain: G → A → C → D
        let network_chain = vec![g, a, c, d];

        (local_dag, network_chain)
    }

    #[test]
    fn test_lca_detection() {
        let (local_dag, network_chain) = build_divergent_dags();
        let b = BlockHash::from_byte(2);

        let result = greedy_path_intersection(&local_dag, &b, &network_chain).unwrap();

        // LCA should be A (byte 1) — the last common block.
        assert_eq!(result.lca, BlockHash::from_byte(1));

        // Missing blocks should be C and D.
        assert_eq!(result.missing_blocks.len(), 2);
        assert!(result.missing_blocks.contains(&BlockHash::from_byte(3)));
        assert!(result.missing_blocks.contains(&BlockHash::from_byte(4)));

        // Divergence depth: local_tip (B) is 1 block past LCA (A).
        assert_eq!(result.divergence_depth, 1);
    }

    #[test]
    fn test_no_divergence() {
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);

        let mut dag = DagStore::new();
        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        color_dag(&mut dag, 3).unwrap();

        let network_chain = vec![g, a];
        let result = greedy_path_intersection(&dag, &a, &network_chain).unwrap();

        // LCA should be A itself — no divergence.
        assert_eq!(result.lca, a);
        assert!(result.missing_blocks.is_empty());
        assert_eq!(result.divergence_depth, 0);
    }

    #[test]
    fn test_has_diverged() {
        let a = BlockHash::from_byte(1);
        let b = BlockHash::from_byte(2);
        let chain = vec![BlockHash::from_byte(0), a];

        assert!(!has_diverged(&a, &chain));
        assert!(has_diverged(&b, &chain));
    }
}
