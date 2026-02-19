"""
Argus Gateway — FastAPI Application.

The Zero-Ops gateway that abstracts the DAG's complexity for
frontend developers and AI consumers.

Endpoints:
    POST /tx/submit-smart  — Smart parent selection for fastest inclusion
    GET  /dag/snapshot      — GNN-ready sub-graph of last N blocks
    GET  /agent/health      — Current k value + RL confidence score

Run:
    uvicorn argus_gateway.main:app --host 0.0.0.0 --port 8000
"""

from __future__ import annotations

import os
from contextlib import asynccontextmanager
from typing import AsyncGenerator

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from argus_gateway.routers import agent, dag, tx
from argus_gateway.services.dag_client import RpcConfig


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncGenerator[None, None]:
    """
    App lifespan: configure RPC clients on startup, clean up on shutdown.
    """
    # Configure RPC clients from environment or defaults.
    rpc_config = RpcConfig(
        host=os.getenv("ARGUS_RPC_HOST", "localhost"),
        port=int(os.getenv("ARGUS_RPC_PORT", "9293")),
        timeout=float(os.getenv("ARGUS_RPC_TIMEOUT", "5.0")),
    )

    tx.configure_client(rpc_config)
    dag.configure_client(rpc_config)
    agent.configure_client(rpc_config)

    print(
        f"[Argus Gateway] Connected to Rust linearizer at "
        f"{rpc_config.host}:{rpc_config.port}"
    )

    yield

    print("[Argus Gateway] Shutting down...")


app = FastAPI(
    title="Argus Orchestration Gateway",
    description=(
        "Zero-Ops agentic gateway for GhostDAG nodes. "
        "Provides linearized DAG access for GNN consumers, "
        "smart transaction submission, and RL-based k-parameter monitoring."
    ),
    version="0.1.0",
    lifespan=lifespan,
)

# CORS middleware — allow all origins for development.
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Mount routers.
app.include_router(tx.router)
app.include_router(dag.router)
app.include_router(agent.router)


@app.get("/", tags=["Root"])
async def root() -> dict[str, str]:
    """Root endpoint — service info."""
    return {
        "service": "Argus Orchestration Gateway",
        "version": "0.1.0",
        "docs": "/docs",
        "endpoints": "/tx/submit-smart, /dag/snapshot, /agent/health",
    }


@app.get("/healthz", tags=["Root"])
async def healthz() -> dict[str, str]:
    """Kubernetes-style liveness probe."""
    return {"status": "ok"}
