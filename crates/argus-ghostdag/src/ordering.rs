/// Argus GhostDAG — PHANTOM Total Ordering.
///
/// Produces a deterministic linear sequence from a DAG by:
///
/// 1. Sorting blocks by `blue_score(B)` ascending.
/// 2. Breaking ties using `hash(B) XOR hash(selected_parent(B))`
///    — lexicographic comparison of the 256-bit XOR result.
///
/// This ordering is the foundation of the Linearization Engine (Task 2)
/// and guarantees GNN consumers receive a deterministic, chronological stream.
use std::cmp::Ordering;

use crate::block::BlockHash;
use crate::dag::DagStore;
use crate::errors::GhostDagResult;

/// A single entry in the linearized output, carrying the block hash
/// and its position in the total order.
#[derive(Debug, Clone)]
pub struct OrderedEntry {
    pub hash: BlockHash,
    pub blue_score: u64,
    pub topological_index: u64,
    /// The XOR tiebreak value (hash XOR selected_parent_hash).
    pub tiebreak_key: BlockHash,
}

/// Produce the PHANTOM total ordering over all blocks in the DAG.
///
/// The DAG **must** have been colored first (via `coloring::color_dag`).
///
/// # Returns
///
/// A `Vec<OrderedEntry>` sorted by `(blue_score ASC, tiebreak_key ASC)`.
pub fn total_order(dag: &DagStore) -> GhostDagResult<Vec<OrderedEntry>> {
    let all_hashes = dag.all_hashes();
    let mut entries: Vec<OrderedEntry> = Vec::with_capacity(all_hashes.len());

    for hash in all_hashes {
        let hdr = dag.get(&hash)?;
        // XOR tiebreak: hash(B) XOR hash(selected_parent(B)).
        // For genesis (no selected parent), use hash XOR ZERO = hash.
        let sp_hash = hdr.selected_parent.unwrap_or(BlockHash::ZERO);
        let tiebreak_key = hash.xor(&sp_hash);

        entries.push(OrderedEntry {
            hash,
            blue_score: hdr.blue_score,
            topological_index: 0, // will be assigned after sorting
            tiebreak_key,
        });
    }

    // Sort: primary key = blue_score ASC, secondary key = tiebreak_key ASC
    // (lexicographic over 256-bit XOR value).
    entries.sort_by(|a, b| {
        a.blue_score
            .cmp(&b.blue_score)
            .then_with(|| a.tiebreak_key.cmp(&b.tiebreak_key))
    });

    // Assign topological indices.
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.topological_index = i as u64;
    }

    Ok(entries)
}

/// Convenience: return just the ordered hashes.
pub fn total_order_hashes(dag: &DagStore) -> GhostDagResult<Vec<BlockHash>> {
    Ok(total_order(dag)?.into_iter().map(|e| e.hash).collect())
}

/// Compare two blocks by the PHANTOM ordering rule.
/// Returns `Ordering::Less` if `a` comes before `b` in the total order.
pub fn compare_blocks(dag: &DagStore, a: &BlockHash, b: &BlockHash) -> GhostDagResult<Ordering> {
    let ha = dag.get(a)?;
    let hb = dag.get(b)?;

    let sp_a = ha.selected_parent.unwrap_or(BlockHash::ZERO);
    let sp_b = hb.selected_parent.unwrap_or(BlockHash::ZERO);

    let xor_a = a.xor(&sp_a);
    let xor_b = b.xor(&sp_b);

    Ok(ha
        .blue_score
        .cmp(&hb.blue_score)
        .then_with(|| xor_a.cmp(&xor_b)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockHash, BlockHeader};
    use crate::coloring::color_dag;
    use crate::dag::DagStore;

    /// Build a 10-block DAG to test ordering:
    ///
    /// ```text
    ///            G
    ///          / | \
    ///         A  B  C
    ///        / \    |
    ///       D   E   F
    ///        \ / \ /
    ///         H   I
    ///          \ /
    ///           J
    /// ```
    fn build_10_block_dag() -> DagStore {
        let mut dag = DagStore::new();
        let hashes: Vec<BlockHash> = (0u8..10).map(BlockHash::from_byte).collect();
        let [g, a, b, c, d, e, f, h, i, j] = [
            hashes[0], hashes[1], hashes[2], hashes[3], hashes[4],
            hashes[5], hashes[6], hashes[7], hashes[8], hashes[9],
        ];

        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(b, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(c, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(d, vec![a], 2)).unwrap();
        dag.add_block(BlockHeader::new(e, vec![a], 2)).unwrap();
        dag.add_block(BlockHeader::new(f, vec![c], 2)).unwrap();
        dag.add_block(BlockHeader::new(h, vec![d, e], 3)).unwrap();
        dag.add_block(BlockHeader::new(i, vec![e, f], 3)).unwrap();
        dag.add_block(BlockHeader::new(j, vec![h, i], 4)).unwrap();
        dag
    }

    #[test]
    fn test_total_order_determinism() {
        let mut dag = build_10_block_dag();
        color_dag(&mut dag, 3).unwrap();

        let order1 = total_order_hashes(&dag).unwrap();
        let order2 = total_order_hashes(&dag).unwrap();

        // Must be identical across runs — deterministic.
        assert_eq!(order1, order2);
    }

    #[test]
    fn test_total_order_covers_all_blocks() {
        let mut dag = build_10_block_dag();
        color_dag(&mut dag, 3).unwrap();

        let order = total_order(&dag).unwrap();
        assert_eq!(order.len(), 10);

        // Each block has a unique topological_index.
        let indices: Vec<u64> = order.iter().map(|e| e.topological_index).collect();
        let expected: Vec<u64> = (0..10).collect();
        assert_eq!(indices, expected);
    }

    #[test]
    fn test_total_order_genesis_first() {
        let mut dag = build_10_block_dag();
        color_dag(&mut dag, 3).unwrap();

        let order = total_order(&dag).unwrap();
        // Genesis has blue_score 0, so it should appear first.
        assert_eq!(order[0].hash, BlockHash::from_byte(0));
    }

    #[test]
    fn test_compare_blocks() {
        let mut dag = build_10_block_dag();
        color_dag(&mut dag, 3).unwrap();

        let g = BlockHash::from_byte(0);
        let j = BlockHash::from_byte(9);
        // Genesis should come before J in total order.
        let cmp = compare_blocks(&dag, &g, &j).unwrap();
        assert_eq!(cmp, std::cmp::Ordering::Less);
    }
}
