/// Argus GhostDAG — DAG Store.
///
/// In-memory directed acyclic graph with parent/child adjacency,
/// topological sorting, and set-theoretic operations (past, future, anticone).
use std::collections::{HashMap, HashSet, VecDeque};

use crate::block::{BlockHash, BlockHeader};
use crate::errors::{GhostDagError, GhostDagResult};

/// The in-memory DAG store.  Thread-safety is handled externally
/// (the agent wraps this in `Arc<RwLock<…>>`).
#[derive(Debug, Clone)]
pub struct DagStore {
    /// Block headers keyed by hash.
    headers: HashMap<BlockHash, BlockHeader>,
    /// Children index: `hash → [child_hashes]`.
    children: HashMap<BlockHash, Vec<BlockHash>>,
    /// The genesis block hash.
    genesis: Option<BlockHash>,
}

impl DagStore {
    /// Create an empty DAG store.
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
            children: HashMap::new(),
            genesis: None,
        }
    }

    /// Returns the genesis hash, if set.
    pub fn genesis(&self) -> Option<BlockHash> {
        self.genesis
    }

    /// Returns the number of blocks in the store.
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Returns `true` if store is empty.
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Check whether a block exists.
    pub fn contains(&self, hash: &BlockHash) -> bool {
        self.headers.contains_key(hash)
    }

    /// Retrieve a header by hash.
    pub fn get(&self, hash: &BlockHash) -> GhostDagResult<&BlockHeader> {
        self.headers
            .get(hash)
            .ok_or(GhostDagError::BlockNotFound(*hash))
    }

    /// Retrieve a mutable header by hash.
    pub fn get_mut(&mut self, hash: &BlockHash) -> GhostDagResult<&mut BlockHeader> {
        self.headers
            .get_mut(hash)
            .ok_or(GhostDagError::BlockNotFound(*hash))
    }

    /// Insert a genesis block.
    pub fn add_genesis(&mut self, header: BlockHeader) -> GhostDagResult<()> {
        if self.genesis.is_some() {
            return Err(GhostDagError::DuplicateBlock(header.hash));
        }
        let hash = header.hash;
        self.headers.insert(hash, header);
        self.children.entry(hash).or_default();
        self.genesis = Some(hash);
        Ok(())
    }

    /// Insert a non-genesis block.  All parents must already be in the store.
    pub fn add_block(&mut self, header: BlockHeader) -> GhostDagResult<()> {
        let hash = header.hash;
        if self.headers.contains_key(&hash) {
            return Err(GhostDagError::DuplicateBlock(hash));
        }
        if header.parents.is_empty() && self.genesis.is_some() {
            return Err(GhostDagError::OrphanBlock(hash));
        }
        // Verify all parents exist.
        for p in &header.parents {
            if !self.headers.contains_key(p) {
                return Err(GhostDagError::MissingParent {
                    child: hash,
                    parent: *p,
                });
            }
        }
        // Update children index.
        for p in &header.parents {
            self.children.entry(*p).or_default().push(hash);
        }
        self.children.entry(hash).or_default();
        self.headers.insert(hash, header);
        Ok(())
    }

    /// Return direct parents.
    pub fn parents(&self, hash: &BlockHash) -> GhostDagResult<&[BlockHash]> {
        Ok(&self.get(hash)?.parents)
    }

    /// Return direct children.
    pub fn children_of(&self, hash: &BlockHash) -> GhostDagResult<&[BlockHash]> {
        self.children
            .get(hash)
            .map(|v| v.as_slice())
            .ok_or(GhostDagError::BlockNotFound(*hash))
    }

    /// **past(B)** — all ancestors of `B` (not including `B` itself).
    /// BFS traversal backward through parent links.
    pub fn past(&self, hash: &BlockHash) -> GhostDagResult<HashSet<BlockHash>> {
        if !self.headers.contains_key(hash) {
            return Err(GhostDagError::BlockNotFound(*hash));
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        // Seed with B's parents.
        for p in &self.get(hash)?.parents {
            if visited.insert(*p) {
                queue.push_back(*p);
            }
        }
        while let Some(cur) = queue.pop_front() {
            if let Ok(hdr) = self.get(&cur) {
                for p in &hdr.parents {
                    if visited.insert(*p) {
                        queue.push_back(*p);
                    }
                }
            }
        }
        visited.remove(hash);
        Ok(visited)
    }

    /// **future(B)** — all descendants of `B` (not including `B` itself).
    /// BFS traversal forward through children links.
    pub fn future(&self, hash: &BlockHash) -> GhostDagResult<HashSet<BlockHash>> {
        if !self.headers.contains_key(hash) {
            return Err(GhostDagError::BlockNotFound(*hash));
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        for c in self.children_of(hash)? {
            if visited.insert(*c) {
                queue.push_back(*c);
            }
        }
        while let Some(cur) = queue.pop_front() {
            if let Ok(ch) = self.children_of(&cur) {
                for c in ch {
                    if visited.insert(*c) {
                        queue.push_back(*c);
                    }
                }
            }
        }
        visited.remove(hash);
        Ok(visited)
    }

    /// **anticone(B)** — all blocks that are neither in `past(B)`,
    /// `future(B)`, nor `B` itself.
    pub fn anticone(&self, hash: &BlockHash) -> GhostDagResult<HashSet<BlockHash>> {
        let past = self.past(hash)?;
        let future = self.future(hash)?;
        let all_hashes: HashSet<BlockHash> = self.headers.keys().copied().collect();
        let mut ac = all_hashes;
        ac.remove(hash);
        for h in &past {
            ac.remove(h);
        }
        for h in &future {
            ac.remove(h);
        }
        Ok(ac)
    }

    /// Return current tips (blocks with no children).
    pub fn tips(&self) -> Vec<BlockHash> {
        self.children
            .iter()
            .filter_map(|(hash, ch)| if ch.is_empty() { Some(*hash) } else { None })
            .collect()
    }

    /// Topological sort (Kahn's algorithm).  Genesis first, tips last.
    pub fn topological_order(&self) -> GhostDagResult<Vec<BlockHash>> {
        let mut in_degree: HashMap<BlockHash, usize> = HashMap::new();
        for (hash, hdr) in &self.headers {
            in_degree.entry(*hash).or_insert(0);
            // Each parent contributes +1 to the child's in_degree — but
            // in_degree is "number of parents", which we already know.
            let _ = hdr; // handled below
        }
        for hdr in self.headers.values() {
            *in_degree.entry(hdr.hash).or_insert(0) = hdr.parents.len();
        }

        let mut queue: VecDeque<BlockHash> = in_degree
            .iter()
            .filter_map(|(h, d)| if *d == 0 { Some(*h) } else { None })
            .collect();

        let mut sorted = Vec::with_capacity(self.headers.len());
        while let Some(cur) = queue.pop_front() {
            sorted.push(cur);
            if let Ok(ch) = self.children_of(&cur) {
                for c in ch {
                    if let Some(deg) = in_degree.get_mut(c) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(*c);
                        }
                    }
                }
            }
        }

        if sorted.len() != self.headers.len() {
            return Err(GhostDagError::Internal(
                "topological sort failed — DAG contains a cycle".to_string(),
            ));
        }
        Ok(sorted)
    }

    /// Return all block hashes.
    pub fn all_hashes(&self) -> Vec<BlockHash> {
        self.headers.keys().copied().collect()
    }

    /// Return an iterator over all headers.
    pub fn headers(&self) -> impl Iterator<Item = &BlockHeader> {
        self.headers.values()
    }
}

impl Default for DagStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockHeader;

    /// Build a small diamond DAG:
    ///
    /// ```text
    ///        G
    ///       / \
    ///      A   B
    ///       \ /
    ///        C
    /// ```
    fn diamond_dag() -> DagStore {
        let mut dag = DagStore::new();
        let g = BlockHash::from_byte(0);
        let a = BlockHash::from_byte(1);
        let b = BlockHash::from_byte(2);
        let c = BlockHash::from_byte(3);

        dag.add_genesis(BlockHeader::genesis(g, 0)).unwrap();
        dag.add_block(BlockHeader::new(a, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(b, vec![g], 1)).unwrap();
        dag.add_block(BlockHeader::new(c, vec![a, b], 2)).unwrap();
        dag
    }

    #[test]
    fn test_past() {
        let dag = diamond_dag();
        let c = BlockHash::from_byte(3);
        let past = dag.past(&c).unwrap();
        assert!(past.contains(&BlockHash::from_byte(0))); // G
        assert!(past.contains(&BlockHash::from_byte(1))); // A
        assert!(past.contains(&BlockHash::from_byte(2))); // B
        assert!(!past.contains(&c));
    }

    #[test]
    fn test_future() {
        let dag = diamond_dag();
        let g = BlockHash::from_byte(0);
        let future = dag.future(&g).unwrap();
        assert!(future.contains(&BlockHash::from_byte(1)));
        assert!(future.contains(&BlockHash::from_byte(2)));
        assert!(future.contains(&BlockHash::from_byte(3)));
    }

    #[test]
    fn test_anticone() {
        let dag = diamond_dag();
        let a = BlockHash::from_byte(1);
        let ac = dag.anticone(&a).unwrap();
        // A's anticone should be {B} — B is not an ancestor or descendant of A.
        assert_eq!(ac.len(), 1);
        assert!(ac.contains(&BlockHash::from_byte(2)));
    }

    #[test]
    fn test_tips() {
        let dag = diamond_dag();
        let tips = dag.tips();
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0], BlockHash::from_byte(3));
    }

    #[test]
    fn test_topological_order() {
        let dag = diamond_dag();
        let order = dag.topological_order().unwrap();
        assert_eq!(order.len(), 4);
        // Genesis must come first.
        assert_eq!(order[0], BlockHash::from_byte(0));
        // C must come last.
        assert_eq!(order[3], BlockHash::from_byte(3));
    }
}
