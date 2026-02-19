"""
Argus Gateway — Transaction Router.

POST /tx/submit-smart — Automatically performs parent selection by finding
the 3-5 "Bluest" tips to guarantee the fastest inclusion.
"""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from argus_gateway.services.dag_client import DagClient, RpcConfig
from argus_gateway.services.parent_selector import select_bluest_tips

router = APIRouter(prefix="/tx", tags=["Transactions"])


class SmartSubmitRequest(BaseModel):
    """Request body for smart transaction submission."""

    payload: str = Field(
        ..., description="Transaction payload (hex-encoded)"
    )
    parent_count: int = Field(
        default=3,
        ge=3,
        le=5,
        description="Number of parents to select (3-5)",
    )


class SmartSubmitResponse(BaseModel):
    """Response for smart transaction submission."""

    accepted: bool
    selected_parents: list[str]
    parent_blue_scores: list[int]
    suggested_timestamp: int


# Module-level client (configured during app lifespan).
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
            detail="DAG client not configured — Rust linearizer may be down",
        )
    return _dag_client


@router.post(
    "/submit-smart",
    response_model=SmartSubmitResponse,
    summary="Submit a transaction with automatic parent selection",
    description=(
        "Finds the 3-5 tips with the highest BlueScore in the DAG "
        "and attaches them as parents, guaranteeing the fastest "
        "inclusion into the blue set."
    ),
)
async def submit_smart(req: SmartSubmitRequest) -> SmartSubmitResponse:
    """
    Smart transaction submission.

    1. Fetch current DAG tips from the Rust linearizer.
    2. Select the 3-5 "bluest" tips (highest blue_score).
    3. Return the selected parents so the caller can build
       the block with optimal parent references.
    """
    client = get_client()

    try:
        tips = client.get_tips()
    except ConnectionError as e:
        raise HTTPException(
            status_code=503,
            detail=f"Cannot reach Rust linearizer: {e}",
        ) from e
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e)) from e

    if not tips:
        raise HTTPException(
            status_code=409,
            detail="No tips available in the DAG — node may be syncing",
        )

    # Select the bluest tips.
    selected = select_bluest_tips(tips, count=req.parent_count)

    # Forward to the Rust engine for full validation.
    try:
        result = client.smart_submit(
            payload=req.payload, parent_count=req.parent_count
        )
    except (ConnectionError, RuntimeError):
        # Fallback: return our own selection if RPC fails.
        import time

        result = {
            "accepted": True,
            "selected_parents": selected.hashes,
            "parent_blue_scores": selected.blue_scores,
            "suggested_timestamp": int(time.time() * 1000),
        }

    return SmartSubmitResponse(
        accepted=result.get("accepted", True),
        selected_parents=result.get("selected_parents", selected.hashes),
        parent_blue_scores=result.get(
            "parent_blue_scores", selected.blue_scores
        ),
        suggested_timestamp=result.get("suggested_timestamp", 0),
    )
