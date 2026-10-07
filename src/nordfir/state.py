"""Original-state (recovery) capture.

Before Nordfir may change anything it must hold a record of how the node was
configured. This module only stores and loads that record; restoring from it
is a later step.
"""

from __future__ import annotations

import dataclasses
import json
import sqlite3
import time
from dataclasses import dataclass

from nordfir.model import HardwareSnapshot


@dataclass(frozen=True)
class OriginalState:
    node: str
    captured_at: float
    governor: str
    scaling_min_khz: int
    scaling_max_khz: int
    hardware_min_khz: int
    hardware_max_khz: int


def original_state_from(snapshot: HardwareSnapshot) -> OriginalState:
    """Derive recovery state from a snapshot, refusing if anything is unknown."""
    cpu = snapshot.cpufreq
    required = {
        "governor": cpu.governor,
        "scaling_min_khz": cpu.scaling_min_khz,
        "scaling_max_khz": cpu.scaling_max_khz,
        "hardware_min_khz": cpu.hardware_min_khz,
        "hardware_max_khz": cpu.hardware_max_khz,
    }
    if not snapshot.cpufreq_available:
        raise ValueError("cpufreq interface is not available")
    missing = [name for name, value in required.items() if value is None]
    if missing:
        raise ValueError("unknown values: " + ", ".join(missing))
    if not (
        cpu.hardware_min_khz <= cpu.scaling_min_khz <= cpu.scaling_max_khz
        <= cpu.hardware_max_khz
    ):
        raise ValueError("frequency limits are inconsistent")
    return OriginalState(node=snapshot.node, captured_at=snapshot.captured_at, **required)


def save_original_state(
    conn: sqlite3.Connection, snapshot: HardwareSnapshot
) -> OriginalState:
    """Store recovery state for the node; never overwrites an active record."""
    state = original_state_from(snapshot)
    try:
        with conn:
            conn.execute(
                "INSERT INTO original_states (node, captured_at, data) VALUES (?, ?, ?)",
                (state.node, state.captured_at, json.dumps(dataclasses.asdict(state))),
            )
    except sqlite3.IntegrityError:
        raise RuntimeError(
            f"original state for {state.node!r} already exists; refusing to overwrite"
        ) from None
    return state


def load_original_state(conn: sqlite3.Connection, node: str) -> OriginalState | None:
    row = conn.execute(
        "SELECT data FROM original_states WHERE node = ? AND retired_at IS NULL", (node,)
    ).fetchone()
    return OriginalState(**json.loads(row[0])) if row else None


def retire_original_state(conn: sqlite3.Connection, node: str) -> bool:
    """Mark the active recovery record as used. Returns False if there was none.

    The row is kept for the record; only its ``retired_at`` is set, which frees
    the node to save a new original state.
    """
    with conn:
        cursor = conn.execute(
            "UPDATE original_states SET retired_at = ? WHERE node = ? AND retired_at IS NULL",
            (time.time(), node),
        )
    return cursor.rowcount > 0
