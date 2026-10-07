"""Read-only observer loop: watch the node and keep a record.

Each cycle stores a hardware snapshot and audits a short ``observation``
event. The loop never plans or changes anything; it is safe to run
continuously and exists to build the history later steps will use.

The loop is bounded (``iterations``), stoppable (a ``threading.Event``), and
gives up after repeated failures instead of spinning silently.
"""

from __future__ import annotations

import math
import sqlite3
import threading
from dataclasses import dataclass

from nordfir.audit import record_audit
from nordfir.db import prune_snapshots, save_snapshot
from nordfir.guards import collect_activity
from nordfir.hardware import collect_hardware

MAX_CONSECUTIVE_ERRORS = 5


@dataclass(frozen=True)
class ObserverSummary:
    cycles: int
    errors: int
    reason: str  # "iterations" | "stopped" | "too many errors"


def observe(
    conn: sqlite3.Connection,
    *,
    interval: float,
    stop: threading.Event,
    iterations: int | None = None,
    protect: tuple[str, ...] = (),
    keep_snapshots: int | None = 1000,
    max_consecutive_errors: int = MAX_CONSECUTIVE_ERRORS,
    **collect_kwargs,
) -> ObserverSummary:
    """Run until ``iterations`` cycles, ``stop`` is set, or too many failures."""
    if not (interval > 0 and math.isfinite(interval)):
        raise ValueError("interval must be a positive, finite number")
    record_audit(conn, "observer_started", interval=interval, iterations=iterations)

    cycles = errors = consecutive = 0
    reason = "stopped"
    while not stop.is_set():
        try:
            _cycle(conn, protect, keep_snapshots, collect_kwargs)
            consecutive = 0
        except Exception as error:  # a bad cycle must not kill the observer
            errors += 1
            consecutive += 1
            record_audit(conn, "observer_error", error=f"{type(error).__name__}: {error}")
            if consecutive >= max_consecutive_errors:
                reason = "too many errors"
                break
        cycles += 1
        if iterations is not None and cycles >= iterations:
            reason = "iterations"
            break
        stop.wait(interval)  # wakes immediately when stop is set

    record_audit(conn, "observer_stopped", reason=reason, cycles=cycles, errors=errors)
    return ObserverSummary(cycles, errors, reason)


def _cycle(conn, protect, keep_snapshots, collect_kwargs) -> None:
    snapshot = collect_hardware(**collect_kwargs)
    snapshot_id = save_snapshot(conn, snapshot)
    activity = collect_activity(collect_kwargs.get("procfs_root", "/proc"), protect)
    record_audit(
        conn,
        "observation",
        snapshot_id=snapshot_id,
        node=snapshot.node,
        governor=snapshot.cpufreq.governor,
        scaling_max_khz=snapshot.cpufreq.scaling_max_khz,
        load_1m=snapshot.load_1m,
        ssh_sessions=activity.ssh_sessions,
        protected_running=activity.protected_running,
    )
    if keep_snapshots is not None:
        prune_snapshots(conn, snapshot.node, keep_snapshots)
