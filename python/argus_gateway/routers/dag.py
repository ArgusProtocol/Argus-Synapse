"""
Argus Gateway — DAG Snapshot Router.

GET /dag/snapshot — Returns a GNN-ready sub-graph of the last N blocks,
including adjacency lists, blue work, and topological indices.
"""

from __future__ import annotations

from typing import Any, Optional

from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel, Field

from argus_gateway.services.dag_client import DagClient, RpcConfig

router = APIRouter(prefix="/dag", tags=["DAG"])


class LinearizedBlockOut(BaseModel):
    """A single block in the linearized output."""

    hash: str
    blue_score: int
    blue_work: str
    topological_index: int
    adjacency_list: list[str]
    is_blue: bool
    selected_parent: Optional[str] = None


class DagSnapshotResponse(BaseModel):
    """GNN-ready sub-graph snapshot."""

    blocks: list[LinearizedBlockOut]
    total_blocks: int
    k: int
    tip: str
    generated_at: int


# Module-level client.
_dag_client: DagClient | None = None


def configure_client(config: RpcConfig) -> None:
    """Configure the DAG client for this router."""
    global _dag_client
    _dag_client = DagClient(config)


def get_client() -> DagClient:
    """Get the configured DAG client."""
    if _dag_client is None:
        raise HTTPException(
            status_code=503,
            detail="DAG client not configured",
        )
    return _dag_client


@router.get(
    "/snapshot",
    response_model=DagSnapshotResponse,
    summary="Get a GNN-ready DAG snapshot",
    description=(
        "Returns the last N blocks from the linearized DAG, "
        "including adjacency lists (parents), blue work, "
        "topological index, and blue/red coloring."
    ),
)
async def get_snapshot(
    n: int = Query(
        default=100,
        ge=1,
        le=10000,
        description="Number of recent blocks to include",
    ),
) -> DagSnapshotResponse:
    """
    Fetch a GNN-ready sub-graph of the last N blocks from the
    Rust Linearization Engine.
    """
    client = get_client()

    try:
        snapshot = client.get_snapshot(n=n)
    except ConnectionError as e:
        raise HTTPException(
            status_code=503,
            detail=f"Cannot reach Rust linearizer: {e}",
        ) from e
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e)) from e

    blocks = [
        LinearizedBlockOut(
            hash=b.get("hash", ""),
            blue_score=b.get("blue_score", 0),
            blue_work=b.get("blue_work", "0"),
            topological_index=b.get("topological_index", 0),
            adjacency_list=b.get("adjacency_list", []),
            is_blue=b.get("is_blue", False),
            selected_parent=b.get("selected_parent"),
        )
        for b in snapshot.get("blocks", [])
    ]

    return DagSnapshotResponse(
        blocks=blocks,
        total_blocks=snapshot.get("total_blocks", len(blocks)),
        k=snapshot.get("k", 0),
        tip=snapshot.get("tip", ""),
        generated_at=snapshot.get("generated_at", 0),
    )
