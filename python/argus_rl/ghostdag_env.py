"""
Argus RL — GhostDAG Gymnasium Environment.

A Reinforcement Learning environment for dynamic k-parameter optimization
in GhostDAG networks.

Observation Space: {current_k, orphan_rate, tip_regression_velocity, network_latency}
Action Space: Discrete(5) → k adjustments {-2, -1, 0, +1, +2}
Reward: R = ω₁·TPS - ω₂·OrphanRate - ω₃·SecurityMargin
"""

from __future__ import annotations

import math
import random
from collections import deque
from dataclasses import dataclass, field
from typing import Any, Optional

import gymnasium as gym
import numpy as np
from gymnasium import spaces


@dataclass
class BlockSim:
    """A simulated block in the DAG."""

    block_id: int
    parents: list[int]
    timestamp: float
    is_blue: bool = True
    blue_score: int = 0


@dataclass
class DagSimulator:
    """
    A lightweight DAG simulator that models GhostDAG behavior
    for the RL environment.  Simulates block creation, coloring,
    and orphan rate.
    """

    k: int = 3
    blocks: dict[int, BlockSim] = field(default_factory=dict)
    tips: set[int] = field(default_factory=set)
    next_id: int = 0
    blue_count: int = 0
    red_count: int = 0
    orphan_count: int = 0
    total_created: int = 0
    recent_latencies: deque[float] = field(
        default_factory=lambda: deque(maxlen=100)
    )

    def reset(self, k: int = 3) -> None:
        """Reset the simulator to genesis state."""
        self.k = k
        self.blocks.clear()
        self.tips.clear()
        self.next_id = 0
        self.blue_count = 0
        self.red_count = 0
        self.orphan_count = 0
        self.total_created = 0
        self.recent_latencies.clear()

        # Create genesis block.
        genesis = BlockSim(
            block_id=0,
            parents=[],
            timestamp=0.0,
            is_blue=True,
            blue_score=0,
        )
        self.blocks[0] = genesis
        self.tips.add(0)
        self.next_id = 1
        self.blue_count = 1

    def simulate_blocks(self, n: int, base_latency: float = 50.0) -> dict[str, float]:
        """
        Simulate n new blocks being added to the DAG.

        Returns metrics: {tps, orphan_rate, avg_latency, tip_count,
                          tip_regression_velocity}.
        """
        blocks_before = len(self.blocks)

        for _ in range(n):
            self._create_block(base_latency)

        blocks_after = len(self.blocks)
        new_blocks = blocks_after - blocks_before

        # Compute metrics.
        orphan_rate = (
            self.orphan_count / max(self.total_created, 1)
        )

        avg_latency = (
            float(np.mean(list(self.recent_latencies)))
            if self.recent_latencies
            else base_latency
        )

        # TPS: blocks per simulated second (assuming 1 block / 100ms).
        tps = new_blocks / max(n * 0.1, 0.1)

        # Tip regression velocity: rate of tip count change.
        tip_regression_velocity = max(0.0, len(self.tips) - self.k) / max(self.k, 1)

        return {
            "tps": tps,
            "orphan_rate": orphan_rate,
            "avg_latency": avg_latency,
            "tip_count": float(len(self.tips)),
            "tip_regression_velocity": tip_regression_velocity,
        }

    def _create_block(self, base_latency: float) -> None:
        """Create a single new block in the DAG."""
        self.total_created += 1

        if not self.tips:
            return

        # Select parents from current tips (1 to min(k+1, len(tips))).
        num_parents = min(random.randint(1, self.k + 1), len(self.tips))
        parent_ids = random.sample(sorted(self.tips), num_parents)

        # Simulate network latency.
        latency = base_latency + random.gauss(0, base_latency * 0.3)
        latency = max(1.0, latency)
        self.recent_latencies.append(latency)

        # Determine coloring: simulate anticone intersection.
        # The wider the DAG (more tips), the more likely a block is red.
        anticone_blue_estimate = max(0, len(self.tips) - num_parents)

        is_blue = anticone_blue_estimate <= self.k

        # Check for orphan: if latency is very high, block might be orphaned.
        is_orphan = latency > base_latency * 3.0

        if is_orphan:
            self.orphan_count += 1
            # Orphaned blocks are always red.
            is_blue = False

        # Compute blue score from best parent.
        best_parent_score = max(
            (self.blocks[p].blue_score for p in parent_ids), default=0
        )

        block = BlockSim(
            block_id=self.next_id,
            parents=parent_ids,
            timestamp=self.next_id * 100.0 + latency,
            is_blue=is_blue,
            blue_score=best_parent_score + (1 if is_blue else 0),
        )

        self.blocks[self.next_id] = block

        # Update tips: remove parents that are no longer tips.
        for p in parent_ids:
            self.tips.discard(p)
        self.tips.add(self.next_id)

        if is_blue:
            self.blue_count += 1
        else:
            self.red_count += 1

        self.next_id += 1


class GhostDagEnv(gym.Env):
    """
    Gymnasium environment for dynamic k-parameter optimization
    in GhostDAG networks.

    Observation Space (Box, 4 dimensions):
        [current_k, orphan_rate, tip_regression_velocity, network_latency]

    Action Space (Discrete, 5 actions):
        0: k -= 2
        1: k -= 1
        2: k += 0 (no change)
        3: k += 1
        4: k += 2

    Reward Function:
        R = ω₁ · TPS - ω₂ · OrphanRate - ω₃ · SecurityMargin

    Where SecurityMargin penalizes k values that are too high (reduces
    confirmation times) or too low (increases orphan rate).
    """

    metadata = {"render_modes": ["human"]}

    def __init__(
        self,
        initial_k: int = 3,
        k_min: int = 1,
        k_max: int = 32,
        blocks_per_step: int = 10,
        max_steps: int = 500,
        omega_tps: float = 1.0,
        omega_orphan: float = 5.0,
        omega_security: float = 0.5,
        base_latency: float = 50.0,
        render_mode: Optional[str] = None,
    ) -> None:
        super().__init__()

        self.initial_k = initial_k
        self.k_min = k_min
        self.k_max = k_max
        self.blocks_per_step = blocks_per_step
        self.max_steps = max_steps
        self.omega_tps = omega_tps
        self.omega_orphan = omega_orphan
        self.omega_security = omega_security
        self.base_latency = base_latency
        self.render_mode = render_mode

        # Action space: 5 discrete actions → k adjustment in {-2, -1, 0, +1, +2}.
        self.action_space = spaces.Discrete(5)

        # Observation space:
        #   [current_k (normalized), orphan_rate, tip_regression_velocity, network_latency (normalized)]
        self.observation_space = spaces.Box(
            low=np.array([0.0, 0.0, 0.0, 0.0], dtype=np.float32),
            high=np.array([1.0, 1.0, 10.0, 1.0], dtype=np.float32),
        )

        self.dag_sim = DagSimulator()
        self.current_step = 0
        self.current_k = initial_k

    def reset(
        self,
        *,
        seed: Optional[int] = None,
        options: Optional[dict[str, Any]] = None,
    ) -> tuple[np.ndarray, dict[str, Any]]:
        """Reset the environment to initial state."""
        super().reset(seed=seed)

        self.current_k = self.initial_k
        self.current_step = 0
        self.dag_sim.reset(k=self.current_k)

        # Run a few initial blocks to seed metrics.
        metrics = self.dag_sim.simulate_blocks(self.blocks_per_step, self.base_latency)

        obs = self._build_observation(metrics)
        info = {"metrics": metrics, "k": self.current_k}

        return obs, info

    def step(
        self, action: int
    ) -> tuple[np.ndarray, float, bool, bool, dict[str, Any]]:
        """
        Execute one step: adjust k, simulate blocks, compute reward.

        Returns: (observation, reward, terminated, truncated, info)
        """
        self.current_step += 1

        # Decode action: {0: -2, 1: -1, 2: 0, 3: +1, 4: +2}
        k_delta = action - 2
        new_k = max(self.k_min, min(self.k_max, self.current_k + k_delta))
        self.current_k = new_k
        self.dag_sim.k = new_k

        # Simulate blocks.
        metrics = self.dag_sim.simulate_blocks(self.blocks_per_step, self.base_latency)

        # Compute reward: R = ω₁·TPS - ω₂·OrphanRate - ω₃·SecurityMargin
        tps = metrics["tps"]
        orphan_rate = metrics["orphan_rate"]

        # Security margin: penalize extreme k values.
        # Too high k → weaker security (attacker has more room).
        # Too low k → more orphans.
        k_normalized = (self.current_k - self.k_min) / max(self.k_max - self.k_min, 1)
        security_margin = abs(k_normalized - 0.3)  # optimal around 30% of range

        reward = (
            self.omega_tps * tps
            - self.omega_orphan * orphan_rate
            - self.omega_security * security_margin
        )

        # Episode termination.
        terminated = False
        truncated = self.current_step >= self.max_steps

        obs = self._build_observation(metrics)
        info = {
            "metrics": metrics,
            "k": self.current_k,
            "reward_components": {
                "tps_reward": self.omega_tps * tps,
                "orphan_penalty": self.omega_orphan * orphan_rate,
                "security_penalty": self.omega_security * security_margin,
            },
        }

        return obs, reward, terminated, truncated, info

    def _build_observation(self, metrics: dict[str, float]) -> np.ndarray:
        """Build the observation vector from current metrics."""
        k_norm = (self.current_k - self.k_min) / max(self.k_max - self.k_min, 1)
        orphan_rate = min(metrics["orphan_rate"], 1.0)
        tip_vel = min(metrics["tip_regression_velocity"], 10.0)
        latency_norm = min(metrics["avg_latency"] / 500.0, 1.0)

        return np.array(
            [k_norm, orphan_rate, tip_vel, latency_norm],
            dtype=np.float32,
        )

    def render(self) -> None:
        """Render environment state to console."""
        if self.render_mode == "human":
            print(
                f"Step {self.current_step:4d} | k={self.current_k:2d} | "
                f"Tips={len(self.dag_sim.tips):3d} | "
                f"Blue={self.dag_sim.blue_count} Red={self.dag_sim.red_count} | "
                f"Orphans={self.dag_sim.orphan_count}"
            )


# Register the environment with Gymnasium.
gym.register(
    id="GhostDag-v0",
    entry_point="argus_rl.ghostdag_env:GhostDagEnv",
)
