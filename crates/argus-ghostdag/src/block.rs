/// Argus GhostDAG — Block primitives.
///
/// Defines `BlockHash` and `BlockHeader`, the fundamental data types
/// for every block in the GhostDAG.
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// BlockHash
// ---------------------------------------------------------------------------

/// A 256-bit block hash, stored as a fixed-size byte array.
/// Ordering is lexicographic over bytes (big-endian interpretation).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockHash(pub [u8; 32]);

impl BlockHash {
    /// The zero hash — used as a sentinel for "no block."
    pub const ZERO: Self = Self([0u8; 32]);

    /// Convenience constructor from a single byte (for testing).
    /// Fills the last byte; all others are zero.
    pub fn from_byte(b: u8) -> Self {
        let mut h = [0u8; 32];
        h[31] = b;
        Self(h)
    }

    /// Convenience constructor from a `u64` value (for testing).
    /// Writes the value in big-endian into the last 8 bytes.
    pub fn from_u64(v: u64) -> Self {
        let mut h = [0u8; 32];
        h[24..32].copy_from_slice(&v.to_be_bytes());
        Self(h)
    }

    /// Bitwise XOR of two hashes.
    pub fn xor(&self, other: &BlockHash) -> BlockHash {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i];
        }
        BlockHash(result)
    }

    /// Returns the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex-encoded hash.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show abbreviated hex for readability.
        let hex = self.to_hex();
        write!(f, "BlockHash({}…{})", &hex[..8], &hex[56..])
    }
}

impl fmt::Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// ---------------------------------------------------------------------------
// BlockHeader
// ---------------------------------------------------------------------------

/// A block header in the GhostDAG, carrying its hash, parent links,
/// and the blue-score / blue-work fields computed by the coloring algorithm.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    /// The hash of this block.
    pub hash: BlockHash,

    /// Direct parent hashes (at least one for non-genesis blocks).
    pub parents: Vec<BlockHash>,

    /// Timestamp in milliseconds since epoch.
    pub timestamp: u64,

    /// **Blue score**: number of blue blocks in `past(B)`.
    /// Computed by the coloring algorithm; defaults to 0 before coloring.
    pub blue_score: u64,

    /// **Blue work**: cumulative proof-of-work contributed by blue blocks
    /// in `past(B)`.  Stored as arbitrary-precision integer.
    pub blue_work: BigUint,

    /// The selected parent — the parent with the highest blue score
    /// (set during coloring).
    pub selected_parent: Option<BlockHash>,
}

impl BlockHeader {
    /// Create a new header.  `blue_score`, `blue_work`, and
    /// `selected_parent` will be set later by the coloring pass.
    pub fn new(hash: BlockHash, parents: Vec<BlockHash>, timestamp: u64) -> Self {
        Self {
            hash,
            parents,
            timestamp,
            blue_score: 0,
            blue_work: BigUint::ZERO,
            selected_parent: None,
        }
    }

    /// Create the genesis block (no parents, score 0).
    pub fn genesis(hash: BlockHash, timestamp: u64) -> Self {
        Self {
            hash,
            parents: Vec::new(),
            timestamp,
            blue_score: 0,
            blue_work: BigUint::from(1u64), // genesis contributes 1 unit of work
            selected_parent: None,
        }
    }

    /// Returns `true` if this is the genesis block (no parents).
    pub fn is_genesis(&self) -> bool {
        self.parents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_hash_xor() {
        let a = BlockHash::from_u64(0xFF00);
        let b = BlockHash::from_u64(0x00FF);
        let c = a.xor(&b);
        assert_eq!(c, BlockHash::from_u64(0xFFFF));
    }

    #[test]
    fn test_block_hash_ordering() {
        let a = BlockHash::from_byte(1);
        let b = BlockHash::from_byte(2);
        assert!(a < b);
    }

    #[test]
    fn test_genesis() {
        let g = BlockHeader::genesis(BlockHash::from_byte(0), 1000);
        assert!(g.is_genesis());
        assert_eq!(g.parents.len(), 0);
    }
}
