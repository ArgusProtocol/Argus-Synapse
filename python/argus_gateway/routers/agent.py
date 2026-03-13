"""
Argus Gateway — Agent Health Router.

GET /agent/health — Returns the current k value, the RL agent's
confidence score, agent state, and DAG statistics.
"""

from __future__ import annotations

from typing import Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from argus_gateway.services.dag_client import DagClient, RpcConfig

router = APIRouter(prefix="/agent", tags=["Agent"])


class AgentHealthResponse(BaseModel):
    """Agent health and status information."""

    current_k: int = Field(description="Current GhostDAG k parameter")
    rl_confidence: float = Field(
        description="RL agent's confidence in current k (0.0 to 1.0)"
    )
    agent_state: str = Field(
        description="Current agent state: SYNCED, DRIFTING, RECOVERING, PARTITIONED"
    )
    tip_blue_score: int = Field(description="Blue score at the current tip")
    total_blocks: int = Field(description="Total blocks in the DAG")
    blue_count: int = Field(description="Number of blue blocks")
    red_count: int = Field(description="Number of red blocks")


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
    "/health",
    response_model=AgentHealthResponse,
    summary="Get agent health and DAG status",
    description=(
        "Returns the current k parameter, the RL agent's confidence score, "
        "the self-healing agent's state, and DAG coloring statistics."
    ),
)
async def get_health() -> AgentHealthResponse:
    """
    Query the Rust Linearization Engine for current agent health,
    k parameter, and RL confidence.
    """
    client = get_client()

    try:
        health = client.get_health()
    except ConnectionError as e:
        raise HTTPException(
            status_code=503,
            detail=f"Cannot reach Rust linearizer: {e}",
        ) from e
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e)) from e

    return AgentHealthResponse(
        current_k=health.get("current_k", 0),
        rl_confidence=health.get("rl_confidence", 0.0),
        agent_state=health.get("agent_state", "UNKNOWN"),
        tip_blue_score=health.get("tip_blue_score", 0),
        total_blocks=health.get("total_blocks", 0),
        blue_count=health.get("blue_count", 0),
        red_count=health.get("red_count", 0),
    )
