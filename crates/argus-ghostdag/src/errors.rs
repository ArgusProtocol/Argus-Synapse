/// Argus GhostDAG — Error types for DAG operations.
use thiserror::Error;

use crate::block::BlockHash;

/// All errors that can arise during DAG construction, coloring, or ordering.
#[derive(Debug, Error, Clone)]
pub enum GhostDagError {
    #[error("block {0} not found in DAG store")]
    BlockNotFound(BlockHash),

    #[error("block {0} already exists in DAG store")]
    DuplicateBlock(BlockHash),

    #[error("parent {parent} of block {child} not found in DAG store")]
    MissingParent { child: BlockHash, parent: BlockHash },

    #[error("block {0} has no parents and is not genesis")]
    OrphanBlock(BlockHash),

    #[error("DAG has no genesis block")]
    NoGenesis,

    #[error("k parameter must be >= 1, got {0}")]
    InvalidK(u64),

    #[error("ordering error: {0}")]
    OrderingError(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type GhostDagResult<T> = Result<T, GhostDagError>;
