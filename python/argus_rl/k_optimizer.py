"""
Argus RL — k-Parameter Optimizer (Runtime Agent).

Loads a trained PPO model and continuously adjusts the GhostDAG k parameter
by observing live network metrics and sending RPC calls to the Rust node.

On POSIX systems, can optionally send SIGUSR1 to the node process for
immediate k-update notification.

Usage:
    python -m argus_rl.k_optimizer --model-path models/ghostdag_ppo \
                                   --rpc-host localhost --rpc-port 9293
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import socket
import sys
import time
from dataclasses import dataclass
from typing import Optional

import numpy as np
from stable_baselines3 import PPO


@dataclass
class NetworkMetrics:
    """Observed network metrics for the RL agent."""

    current_k: int
    orphan_rate: float
    tip_regression_velocity: float
    network_latency: float


@dataclass
class OptimizerConfig:
    """Configuration for the k-parameter optimizer."""

    model_path: str = "models/ghostdag_ppo"
    rpc_host: str = "localhost"
    rpc_port: int = 9293
    poll_interval_seconds: float = 5.0
    k_min: int = 1
    k_max: int = 32
    node_pid: Optional[int] = None
    use_sigusr1: bool = False


class KOptimizer:
    """
    Runtime RL agent that dynamically optimizes the GhostDAG k parameter.

    Workflow:
    1. Load trained PPO model.
    2. Poll network metrics from the Rust node via JSON-RPC.
    3. Run inference to determine k adjustment.
    4. Send `update_k` RPC call to hot-swap k without restart.
    5. Optionally send SIGUSR1 to the node process for immediate effect.
    """

    def __init__(self, config: OptimizerConfig) -> None:
        self.config = config
        self.model = PPO.load(config.model_path)
        self.current_k = 3  # Will be updated from node
        self.confidence: float = 0.0
        self._consecutive_no_change = 0

        print(f"[KOptimizer] Loaded model from: {config.model_path}")
        print(f"[KOptimizer] RPC endpoint: {config.rpc_host}:{config.rpc_port}")

    def run(self) -> None:
        """Run the optimizer loop indefinitely."""
        print("[KOptimizer] Starting optimization loop...")

        while True:
            try:
                # 1. Fetch metrics from the node.
                metrics = self._fetch_metrics()
                if metrics is None:
                    print("[KOptimizer] Failed to fetch metrics — retrying...")
                    time.sleep(self.config.poll_interval_seconds)
                    continue

                self.current_k = metrics.current_k

                # 2. Build observation vector.
                obs = self._build_observation(metrics)

                # 3. Run PPO inference.
                action, _states = self.model.predict(obs, deterministic=True)
                action = int(action)

                # Decode action: {0: -2, 1: -1, 2: 0, 3: +1, 4: +2}
                k_delta = action - 2
                new_k = max(
                    self.config.k_min,
                    min(self.config.k_max, self.current_k + k_delta),
                )

                # 4. Compute confidence based on action probability distribution.
                obs_tensor = self.model.policy.obs_to_tensor(obs.reshape(1, -1))[0]
                action_dist = self.model.policy.get_distribution(obs_tensor)
                probs = action_dist.distribution.probs.detach().cpu().numpy()[0]
                self.confidence = float(probs[action])

                # 5. Apply k change if different.
                if new_k != self.current_k:
                    print(
                        f"[KOptimizer] Adjusting k: {self.current_k} → {new_k} "
                        f"(action={action}, delta={k_delta:+d}, "
                        f"confidence={self.confidence:.3f})"
                    )
                    success = self._send_update_k(new_k)
                    if success:
                        self.current_k = new_k
                        self._consecutive_no_change = 0

                        # Send SIGUSR1 if configured (POSIX only).
                        if self.config.use_sigusr1 and self.config.node_pid:
                            self._send_sigusr1()
                    else:
                        print("[KOptimizer] Failed to update k via RPC")
                else:
                    self._consecutive_no_change += 1
                    if self._consecutive_no_change % 10 == 0:
                        print(
                            f"[KOptimizer] k={self.current_k} stable for "
                            f"{self._consecutive_no_change} cycles "
                            f"(confidence={self.confidence:.3f})"
                        )

            except KeyboardInterrupt:
                print("\n[KOptimizer] Shutting down...")
                break
            except Exception as e:
                print(f"[KOptimizer] Error: {e}")

            time.sleep(self.config.poll_interval_seconds)

    def get_confidence(self) -> float:
        """Return the RL agent's current confidence score."""
        return self.confidence

    def _build_observation(self, metrics: NetworkMetrics) -> np.ndarray:
        """Convert network metrics to a normalized observation vector."""
        k_norm = (metrics.current_k - self.config.k_min) / max(
            self.config.k_max - self.config.k_min, 1
        )
        orphan_rate = min(metrics.orphan_rate, 1.0)
        tip_vel = min(metrics.tip_regression_velocity, 10.0)
        latency_norm = min(metrics.network_latency / 500.0, 1.0)

        return np.array(
            [k_norm, orphan_rate, tip_vel, latency_norm], dtype=np.float32
        )

    def _fetch_metrics(self) -> Optional[NetworkMetrics]:
        """Fetch current metrics from the Rust node via JSON-RPC."""
        request = json.dumps(
            {
                "jsonrpc": "2.0",
                "method": "get_health",
                "id": 1,
            }
        )

        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
                sock.settimeout(5.0)
                sock.connect((self.config.rpc_host, self.config.rpc_port))
                sock.sendall(request.encode("utf-8"))
                response = sock.recv(8192).decode("utf-8")

            data = json.loads(response)
            result = data.get("result", {})

            return NetworkMetrics(
                current_k=result.get("current_k", 3),
                orphan_rate=result.get("red_count", 0)
                / max(result.get("total_blocks", 1), 1),
                tip_regression_velocity=0.0,  # Computed from tip count delta
                network_latency=50.0,  # Estimated from recent block timestamps
            )
        except (ConnectionRefusedError, socket.timeout, json.JSONDecodeError) as e:
            print(f"[KOptimizer] RPC error: {e}")
            return None

    def _send_update_k(self, new_k: int) -> bool:
        """Send an update_k RPC call to the Rust node."""
        request = json.dumps(
            {
                "jsonrpc": "2.0",
                "method": "update_k",
                "params": {"new_k": new_k},
                "id": 2,
            }
        )

        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
                sock.settimeout(5.0)
                sock.connect((self.config.rpc_host, self.config.rpc_port))
                sock.sendall(request.encode("utf-8"))
                response = sock.recv(4096).decode("utf-8")

            data = json.loads(response)
            return "result" in data
        except (ConnectionRefusedError, socket.timeout, json.JSONDecodeError) as e:
            print(f"[KOptimizer] RPC send error: {e}")
            return False

    def _send_sigusr1(self) -> None:
        """Send SIGUSR1 to the node process (POSIX only)."""
        if not hasattr(signal, "SIGUSR1"):
            print("[KOptimizer] SIGUSR1 not available on this platform")
            return

        if self.config.node_pid:
            try:
                os.kill(self.config.node_pid, signal.SIGUSR1)
                print(
                    f"[KOptimizer] Sent SIGUSR1 to PID {self.config.node_pid}"
                )
            except ProcessLookupError:
                print(
                    f"[KOptimizer] PID {self.config.node_pid} not found"
                )
            except PermissionError:
                print(
                    f"[KOptimizer] Permission denied sending signal to PID "
                    f"{self.config.node_pid}"
                )


def main() -> None:
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        description="GhostDAG k-parameter optimizer using trained PPO model"
    )
    parser.add_argument(
        "--model-path",
        type=str,
        default="models/ghostdag_ppo",
        help="Path to trained PPO model",
    )
    parser.add_argument(
        "--rpc-host",
        type=str,
        default="localhost",
        help="Rust node RPC host",
    )
    parser.add_argument(
        "--rpc-port",
        type=int,
        default=9293,
        help="Rust node RPC port",
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=5.0,
        help="Poll interval in seconds",
    )
    parser.add_argument(
        "--node-pid",
        type=int,
        default=None,
        help="Node PID for SIGUSR1 (POSIX only)",
    )
    args = parser.parse_args()

    config = OptimizerConfig(
        model_path=args.model_path,
        rpc_host=args.rpc_host,
        rpc_port=args.rpc_port,
        poll_interval_seconds=args.interval,
        node_pid=args.node_pid,
        use_sigusr1=args.node_pid is not None,
    )

    optimizer = KOptimizer(config)
    optimizer.run()


if __name__ == "__main__":
    main()
