/// Argus GhostDAG — k-Cluster Coloring Algorithm.
///
/// Implements the PHANTOM k-cluster coloring from §3 of the original paper:
///
/// 1. Process blocks in topological order.
/// 2. For each block B, compute `anticone(B) ∩ blue_set`.
/// 3. If `|anticone(B) ∩ blue_set| ≤ k`, color B blue; otherwise red.
/// 4. `selected_parent(B)` = parent with highest blue score.
/// 5. `blue_score(B)` = `|blue_set ∩ past(B)|`.
///
/// This module mutates `BlockHeader.blue_score`, `BlockHeader.selected_parent`,
/// and `BlockHeader.blue_work` in-place on the `DagStore`.
use std::collections::HashSet;

use num_bigint::BigUint;

use crate::block::BlockHash;
use crate::dag::DagStore;
use crate::errors::{GhostDagError, GhostDagResult};

/// Result of a coloring pass over the entire DAG.
#[derive(Debug, Clone)]
pub struct ColoringOutput {
    /// Blocks colored blue.
    pub blue_set: HashSet<BlockHash>,
    /// Blocks colored red.
    pub red_set: HashSet<BlockHash>,
    /// The k parameter used.
    pub k: u64,
}

/// Compute `|anticone(B) ∩ blue_set|` for a given block after coloring.
pub fn blue_anticone_size(
    dag: &mut DagStore,
    hash: &BlockHash,
    blue_set: &HashSet<BlockHash>,
) -> GhostDagResult<u64> {
    let ac = dag.anticone(hash)?;
    Ok(ac.iter().filter(|h| blue_set.contains(h)).count() as u64)
}

/// Run the k-cluster coloring algorithm over the entire DAG.
///
/// # Algorithm (PHANTOM §3)
///
/// ```text
/// blue_set ← {genesis}
/// for B in topological_order \ {genesis}:
///     // 1.  selected_parent(B) ← argmax_{P ∈ parents(B)} blue_score(P)
///     // 2.  blue_anticone_size ← |anticone(B) ∩ blue_set|
///     // 3.  if blue_anticone_size ≤ k:  color B blue
///     //     else:                         color B red
///     // 4.  blue_score(B) ← |blue_set ∩ past(B)|
///     // 5.  blue_work(B) ← blue_work(selected_parent(B)) + (1 if B is blue else 0)
/// ```
pub fn color_dag(dag: &mut DagStore, k: u64) -> GhostDagResult<ColoringOutput> {
    if k < 1 {
        return Err(GhostDagError::InvalidK(k));
    }
    let genesis = dag.genesis().ok_or(GhostDagError::NoGenesis)?;
    let topo = dag.topological_order()?;

    let mut blue_set: HashSet<BlockHash> = HashSet::new();
    let mut red_set: HashSet<BlockHash> = HashSet::new();

    // Genesis is always blue.
    blue_set.insert(genesis);

    for &block_hash in &topo {
        if block_hash == genesis {
            // Genesis already handled — just set its score fields.
            let g = dag.get_mut(&genesis)?;
            g.blue_score = 0;
            g.blue_work = BigUint::from(1u64);
            g.selected_parent = None;
            continue;
        }

        // -----------------------------------------------------------
        // Step 1: selected_parent(B) = argmax_{P ∈ parents(B)} blue_score(P)
        // -----------------------------------------------------------
        let selected_parent = {
            let header = dag.get(&block_hash)?;
            let parents = header.parents.clone();
            parents
                .iter()
                .max_by(|a, b| {
                    let score_a = dag.get(a).map(|h| h.blue_score).unwrap_or(0);
                    let score_b = dag.get(b).map(|h| h.blue_score).unwrap_or(0);
                    score_a.cmp(&score_b).then_with(|| a.cmp(b))
                })
                .copied()
                .ok_or(GhostDagError::OrphanBlock(block_hash))?
        };

        // -----------------------------------------------------------
        // Step 2: Compute blue anticone size.
        // -----------------------------------------------------------
        let blue_anticone_count = {
            let ac = dag.anticone(&block_hash)?;
            ac.iter().filter(|h| blue_set.contains(h)).count() as u64
        };

        // -----------------------------------------------------------
        // Step 3: Color decision.
        // -----------------------------------------------------------
        let is_blue = blue_anticone_count <= k;
        if is_blue {
            blue_set.insert(block_hash);
        } else {
            red_set.insert(block_hash);
        }

        // -----------------------------------------------------------
        // Step 4: blue_score(B) = |blue_set ∩ past(B)|
        // -----------------------------------------------------------
        let blue_score = {
            let past = dag.past(&block_hash)?;
            past.iter().filter(|h| blue_set.contains(h)).count() as u64
        };

        // -----------------------------------------------------------
        // Step 5: blue_work(B)
        // -----------------------------------------------------------
        let parent_blue_work = dag.get(&selected_parent)?.blue_work.clone();
        let blue_work = if is_blue {
            parent_blue_work + BigUint::from(1u64)
        } else {
            parent_blue_work
        };

        // Write back.
        let header = dag.get_mut(&block_hash)?;
        header.selected_parent = Some(selected_parent);
        header.blue_score = blue_score;
        header.blue_work = blue_work;
    }

    Ok(ColoringOutput { blue_set, red_set, k })
}

/// Compute the selected-parent chain from `tip` back to genesis.
pub fn selected_parent_chain(
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

/// Compute `|anticone(B) ∩ blue_set|` for a given block after coloring.
pub fn blue_anticone_size(
    dag: &DagStore,
    hash: &BlockHash,
    blue_set: &HashSet<BlockHash>,
) -> GhostDagResult<u64> {
    let ac = dag.anticone(hash)?;
    Ok(ac.iter().filter(|h| blue_set.contains(h)).count() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockHash, BlockHeader};

    /// Build a 7-block DAG for coloring verification:
    ///
    /// ```text
    ///           G (genesis)
    ///          / \
    ///         A   B
    ///        /|   |\
    ///       C D   E F
    ///        \   /
    ///         \ /
    ///          H
    /// ```
    ///
    /// With k=1, blocks in linear chains stay blue; blocks creating
    /// wide anticones turn red.
    fn build_test_dag() -> DagStore {
        let mut dag = DagStore::new();
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);
        let b = BlockHash::from_byte(2);
        let c = BlockHash::from_byte(3);
        let d = BlockHash::from_byte(4);
        let e = BlockHash::from_byte(5);
        let f = BlockHash::from_byte(6);
        let h = BlockHash::from_byte(7);

        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(b, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(c, vec![a], 2)).unwrap();
        dag.add_block(BlockHeader::new(d, vec![a], 2)).unwrap();
        dag.add_block(BlockHeader::new(e, vec![b], 2)).unwrap();
        dag.add_block(BlockHeader::new(f, vec![b], 2)).unwrap();
        dag.add_block(BlockHeader::new(h, vec![c, e], 3)).unwrap();
        dag
    }

    #[test]
    fn test_coloring_k1() {
        let mut dag = build_test_dag();
        let result = color_dag(&mut dag, 1).unwrap();

        // Genesis is always blue.
        assert!(result.blue_set.contains(&BlockHash::from_byte(0)));

        // Total blocks should equal blue + red.
        assert_eq!(result.blue_set.len() + result.red_set.len(), 8);

        // With k=1 the blue set should be reasonably small
        // (blocks with large anticone intersection get colored red).
        assert!(result.blue_set.len() >= 3); // at least genesis + 2
    }

    #[test]
    fn test_coloring_high_k() {
        let mut dag = build_test_dag();
        let result = color_dag(&mut dag, 100).unwrap();

        // With very high k, ALL blocks should be blue —
        // no anticone can exceed k.
        assert_eq!(result.blue_set.len(), 8);
        assert!(result.red_set.is_empty());
    }

    #[test]
    fn test_selected_parent_chain() {
        let mut dag = build_test_dag();
        color_dag(&mut dag, 2).unwrap();

        let h = BlockHash::from_byte(7);
        let chain = selected_parent_chain(&dag, &h).unwrap();
        // Chain must start at genesis and end at H.
        assert_eq!(*chain.first().unwrap(), BlockHash::from_byte(0));
        assert_eq!(*chain.last().unwrap(), h);
    }
}
