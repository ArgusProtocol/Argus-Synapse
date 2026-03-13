/// Argus GhostDAG — Core DAG math library.
///
/// This crate provides the foundational GhostDAG primitives:
///
/// - **`block`** — `BlockHash` and `BlockHeader` types.
/// - **`dag`** — `DagStore` with parent/child adjacency, `past(B)`,
///   `future(B)`, `anticone(B)`, and topological sorting.
/// - **`coloring`** — k-cluster coloring algorithm per PHANTOM §3.
/// - **`ordering`** — PHANTOM total ordering with XOR tiebreak.
/// - **`errors`** — Error types.

pub mod block;
pub mod coloring;
pub mod dag;
pub mod errors;
pub mod ordering;

// Re-exports for convenience.
pub use block::{BlockHash, BlockHeader};
pub use coloring::{color_dag, selected_parent_chain, ColoringOutput};
pub use dag::DagStore;
pub use errors::{GhostDagError, GhostDagResult};
pub use ordering::{total_order, total_order_hashes, OrderedEntry};
