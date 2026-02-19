"""
Argus Gateway — Smart Parent Selector.

Selects the 3-5 "bluest" DAG tips for transaction parent selection,
guaranteeing fastest inclusion into the blue set.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class SelectedParents:
    """Result of parent selection."""

    parents: list[dict[str, Any]]
    count: int

    @property
    def hashes(self) -> list[str]:
        return [p["hash"] for p in self.parents]

    @property
    def blue_scores(self) -> list[int]:
        return [p["blue_score"] for p in self.parents]


def select_bluest_tips(
    tips: list[dict[str, Any]],
    count: int = 3,
) -> SelectedParents:
    """
    Select the `count` tips with the highest blue scores.

    Args:
        tips: List of tip dicts with "hash" and "blue_score" keys.
        count: Number of parents to select (3-5, clamped).

    Returns:
        SelectedParents with the chosen tips.
    """
    count = max(3, min(5, count))

    # Sort by blue_score descending, then by hash for determinism.
    sorted_tips = sorted(
        tips,
        key=lambda t: (-t.get("blue_score", 0), t.get("hash", "")),
    )

    selected = sorted_tips[:count]

    return SelectedParents(parents=selected, count=len(selected))
