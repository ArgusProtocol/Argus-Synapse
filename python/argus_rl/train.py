"""
Argus RL — PPO Training Script.

Trains a PPO agent using Stable-Baselines3 on the GhostDag-v0 environment
for dynamic k-parameter optimization.

Usage:
    python -m argus_rl.train [--timesteps N] [--save-path PATH]
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

import numpy as np
from stable_baselines3 import PPO
from stable_baselines3.common.callbacks import (
    BaseCallback,
    CheckpointCallback,
    EvalCallback,
)
from stable_baselines3.common.env_util import make_vec_env
from stable_baselines3.common.monitor import Monitor

from argus_rl.ghostdag_env import GhostDagEnv


class KTrackingCallback(BaseCallback):
    """
    Custom callback that logs k-parameter values and reward components
    during training for analysis.
    """

    def __init__(self, verbose: int = 0) -> None:
        super().__init__(verbose)
        self.k_history: list[int] = []
        self.reward_history: list[float] = []

    def _on_step(self) -> bool:
        # Extract info from the environment.
        infos = self.locals.get("infos", [])
        for info in infos:
            if "k" in info:
                self.k_history.append(info["k"])
            if "reward_components" in info:
                self.reward_history.append(
                    info["reward_components"].get("tps_reward", 0.0)
                )
        return True

    def _on_training_end(self) -> None:
        if self.k_history:
            avg_k = np.mean(self.k_history[-1000:])
            print(f"\n[KTracker] Average k over last 1000 steps: {avg_k:.2f}")
            print(
                f"[KTracker] k range: [{min(self.k_history)}, {max(self.k_history)}]"
            )


def make_env(
    initial_k: int = 3,
    blocks_per_step: int = 10,
    max_steps: int = 500,
) -> GhostDagEnv:
    """Create a monitored GhostDag environment."""
    env = GhostDagEnv(
        initial_k=initial_k,
        blocks_per_step=blocks_per_step,
        max_steps=max_steps,
    )
    return env


def train(
    total_timesteps: int = 100_000,
    save_path: str = "models/ghostdag_ppo",
    initial_k: int = 3,
    learning_rate: float = 3e-4,
    n_steps: int = 2048,
    batch_size: int = 64,
    n_epochs: int = 10,
    gamma: float = 0.99,
    verbose: int = 1,
) -> PPO:
    """
    Train a PPO agent on the GhostDag-v0 environment.

    Args:
        total_timesteps: Number of timesteps to train for.
        save_path: Where to save the trained model.
        initial_k: Starting k parameter.
        learning_rate: PPO learning rate.
        n_steps: Steps per rollout.
        batch_size: Mini-batch size.
        n_epochs: Number of optimization epochs per update.
        gamma: Discount factor.
        verbose: Verbosity level.

    Returns:
        The trained PPO model.
    """
    print(f"=== Argus RL — PPO Training ===")
    print(f"  Total timesteps:  {total_timesteps:,}")
    print(f"  Initial k:        {initial_k}")
    print(f"  Learning rate:    {learning_rate}")
    print(f"  Save path:        {save_path}")
    print()

    # Create training environment.
    env = make_env(initial_k=initial_k)
    env = Monitor(env)

    # Create evaluation environment.
    eval_env = make_env(initial_k=initial_k)
    eval_env = Monitor(eval_env)

    # Ensure save directory exists.
    os.makedirs(os.path.dirname(save_path) if os.path.dirname(save_path) else "models", exist_ok=True)

    # Callbacks.
    k_tracker = KTrackingCallback(verbose=verbose)

    checkpoint_cb = CheckpointCallback(
        save_freq=max(total_timesteps // 10, 1000),
        save_path=os.path.dirname(save_path) if os.path.dirname(save_path) else "models",
        name_prefix="ghostdag_ppo_ckpt",
    )

    eval_cb = EvalCallback(
        eval_env,
        best_model_save_path=os.path.dirname(save_path) if os.path.dirname(save_path) else "models",
        log_path=os.path.dirname(save_path) if os.path.dirname(save_path) else "models",
        eval_freq=max(total_timesteps // 20, 500),
        n_eval_episodes=5,
        deterministic=True,
        verbose=verbose,
    )

    # Create PPO agent.
    model = PPO(
        "MlpPolicy",
        env,
        learning_rate=learning_rate,
        n_steps=n_steps,
        batch_size=batch_size,
        n_epochs=n_epochs,
        gamma=gamma,
        verbose=verbose,
        tensorboard_log=os.path.join(
            os.path.dirname(save_path) if os.path.dirname(save_path) else "models",
            "tb_logs",
        ),
    )

    # Train.
    print("Starting training...\n")
    model.learn(
        total_timesteps=total_timesteps,
        callback=[k_tracker, checkpoint_cb, eval_cb],
        progress_bar=True,
    )

    # Save the final model.
    model.save(save_path)
    print(f"\nModel saved to: {save_path}")

    # Print summary.
    print("\n=== Training Complete ===")
    if k_tracker.k_history:
        final_k_avg = np.mean(k_tracker.k_history[-500:])
        print(f"  Final average k (last 500 steps): {final_k_avg:.2f}")

    env.close()
    eval_env.close()

    return model


def main() -> None:
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        description="Train a PPO agent for GhostDAG k-parameter optimization"
    )
    parser.add_argument(
        "--timesteps",
        type=int,
        default=100_000,
        help="Total training timesteps (default: 100000)",
    )
    parser.add_argument(
        "--save-path",
        type=str,
        default="models/ghostdag_ppo",
        help="Path to save the trained model",
    )
    parser.add_argument(
        "--initial-k",
        type=int,
        default=3,
        help="Initial k parameter (default: 3)",
    )
    parser.add_argument(
        "--lr",
        type=float,
        default=3e-4,
        help="Learning rate (default: 3e-4)",
    )
    args = parser.parse_args()

    train(
        total_timesteps=args.timesteps,
        save_path=args.save_path,
        initial_k=args.initial_k,
        learning_rate=args.lr,
    )


if __name__ == "__main__":
    main()
