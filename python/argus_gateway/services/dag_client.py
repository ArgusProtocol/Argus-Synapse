"""
Argus Gateway — DAG RPC Client.

Communicates with the Rust Linearization Engine via JSON-RPC
over TCP (port 9293 by default).
"""

from __future__ import annotations

import json
import socket
from dataclasses import dataclass
from typing import Any, Optional


@dataclass
class RpcConfig:
    """Configuration for the JSON-RPC client."""

    host: str = "localhost"
    port: int = 9293
    timeout: float = 5.0


class DagClient:
    """
    JSON-RPC client for the Argus Linearizer.

    Provides typed methods for each RPC endpoint exposed by the
    Rust server.
    """

    def __init__(self, config: Optional[RpcConfig] = None) -> None:
        self.config = config or RpcConfig()
        self._request_id = 0

    def _next_id(self) -> int:
        self._request_id += 1
        return self._request_id

    def _call(self, method: str, params: Optional[dict[str, Any]] = None) -> Any:
        """
        Send a JSON-RPC 2.0 request and return the result.

        Raises:
            ConnectionError: If the RPC server is unreachable.
            RuntimeError: If the RPC returns an error.
        """
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "id": self._next_id(),
        }
        if params is not None:
            request["params"] = params

        payload = json.dumps(request).encode("utf-8")

        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
                sock.settimeout(self.config.timeout)
                sock.connect((self.config.host, self.config.port))
                sock.sendall(payload)

                # Read response (up to 64KB).
                chunks: list[bytes] = []
                while True:
                    try:
                        chunk = sock.recv(65536)
                        if not chunk:
                            break
                        chunks.append(chunk)
                        # Try to parse — if valid JSON, we're done.
                        try:
                            json.loads(b"".join(chunks))
                            break
                        except json.JSONDecodeError:
                            continue
                    except socket.timeout:
                        break

                raw = b"".join(chunks).decode("utf-8")
        except (ConnectionRefusedError, OSError) as e:
            raise ConnectionError(
                f"Cannot connect to Argus Linearizer at "
                f"{self.config.host}:{self.config.port}: {e}"
            ) from e

        if not raw:
            raise RuntimeError("Empty response from RPC server")

        data = json.loads(raw)

        if "error" in data and data["error"] is not None:
            err = data["error"]
            raise RuntimeError(
                f"RPC error {err.get('code', '?')}: {err.get('message', 'unknown')}"
            )

        return data.get("result")

    # ---- Typed methods ----

    def get_tips(self) -> list[dict[str, Any]]:
        """Get current DAG tips with blue scores."""
        return self._call("get_tips")

    def get_tip_order(self) -> list[dict[str, Any]]:
        """Get the full PHANTOM total ordering."""
        return self._call("get_tip_order")

    def get_snapshot(self, n: int = 100) -> dict[str, Any]:
        """Get a GNN-ready snapshot of the last N blocks."""
        return self._call("get_snapshot", {"n": n})

    def get_health(self) -> dict[str, Any]:
        """Get agent health info."""
        return self._call("get_health")

    def linearize_range(self, from_score: int, to_score: int) -> list[dict[str, Any]]:
        """Get blocks in a blue-score range."""
        return self._call(
            "linearize_range", {"from_score": from_score, "to_score": to_score}
        )

    def update_k(self, new_k: int) -> dict[str, Any]:
        """Hot-swap the k parameter."""
        return self._call("update_k", {"new_k": new_k})

    def smart_submit(
        self, payload: str, parent_count: int = 3
    ) -> dict[str, Any]:
        """Submit a transaction with automatic parent selection."""
        return self._call(
            "smart_submit",
            {"payload": payload, "parent_count": parent_count},
        )
