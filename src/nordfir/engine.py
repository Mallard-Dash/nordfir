# This is the start of the Nordfir project 24th september 2026 by Vincent Bergengren
"""The single decision engine: observe -> decide -> record.

The engine only *decides*. Applying a desired state to the host belongs to a
future driver module and is deliberately not implemented yet.
"""

from __future__ import annotations

import sqlite3

from nordfir.audit import record_audit
from nordfir.db import save_snapshot
from nordfir.driver import ApplyResult, Driver
from nordfir.hardware import collect_hardware
from nordfir.model import DesiredState, HardwareSnapshot, Intent, PowerMode
from nordfir.plan import ChangePlan, action_to_dict, plan_restore
from nordfir.state import load_original_state, retire_original_state

REST_GOVERNOR = "powersave"
REST_CPU_MAX_PERCENT = 40


def desired_state(snapshot: HardwareSnapshot, intent: Intent) -> DesiredState:
    """Work out the target power state for ``intent``, failing closed on unknowns."""
    if intent is Intent.AVAILABLE:
        return DesiredState(intent, PowerMode.ACTIVE)
    if intent is Intent.MAINTENANCE:
        # Maintenance suppresses optimization: leave the node as it is.
        return DesiredState(intent, mode=None)
    if intent is Intent.RELEASE:
        return DesiredState(intent, None, blockers=("OFF is not supported yet",))
    return _rest_state(snapshot, intent)


def _rest_state(snapshot: HardwareSnapshot, intent: Intent) -> DesiredState:
    cpu = snapshot.cpufreq
    blockers = []
    if not snapshot.cpufreq_available:
        blockers.append("cpufreq interface is not available")
    if cpu.governor is None:
        blockers.append("current governor is unknown")
    if REST_GOVERNOR not in cpu.available_governors:
        blockers.append(f"governor {REST_GOVERNOR!r} is not available")
    if cpu.hardware_min_khz is None or cpu.hardware_max_khz is None:
        blockers.append("hardware frequency range is unknown")
    if cpu.scaling_max_khz is None:
        blockers.append("current maximum frequency is unknown")
    if blockers:
        return DesiredState(intent, PowerMode.REST, blockers=tuple(blockers))

    ceiling = cpu.hardware_max_khz * REST_CPU_MAX_PERCENT // 100
    ceiling = max(cpu.hardware_min_khz, min(ceiling, cpu.hardware_max_khz))
    # Never raise a ceiling that is already more restrictive.
    ceiling = min(ceiling, cpu.scaling_max_khz)
    return DesiredState(
        intent, PowerMode.REST, governor=REST_GOVERNOR, scaling_max_khz=ceiling
    )


def run_once(conn: sqlite3.Connection, intent: Intent, **collect_kwargs) -> DesiredState:
    """One observe/decide cycle. Records the snapshot and decision; changes nothing."""
    snapshot = collect_hardware(**collect_kwargs)
    snapshot_id = save_snapshot(conn, snapshot)
    desired = desired_state(snapshot, intent)
    record_audit(
        conn,
        "decision",
        snapshot_id=snapshot_id,
        intent=desired.intent.value,
        mode=desired.mode.value if desired.mode else None,
        governor=desired.governor,
        scaling_max_khz=desired.scaling_max_khz,
        blockers=list(desired.blockers),
    )
    return desired


def execute_plan(
    conn: sqlite3.Connection, driver: Driver, plan: ChangePlan, *, operation: str = "apply"
) -> ApplyResult | None:
    """Hand an unblocked plan to ``driver`` and audit the outcome.

    Events are named after ``operation``: ``<op>_started``, ``<op>`` and
    ``<op>_refused`` (so a restore is never mistaken for an apply).

    Returns ``None`` (and audits the refusal) for a blocked plan; the driver is
    never called in that case.
    """
    if plan.blocked:
        record_audit(
            conn, f"{operation}_refused", node=plan.node, driver=driver.name,
            blockers=list(plan.blockers), deferrals=list(plan.deferrals),
        )
        return None
    # Record the intent first so a crash mid-write still leaves a trace.
    record_audit(
        conn, f"{operation}_started", node=plan.node, driver=driver.name,
        actions=[action_to_dict(action) for action in plan.actions],
    )
    result = driver.apply(plan)
    record_audit(
        conn, operation, node=plan.node, driver=result.driver, applied=result.applied,
        error=result.error, rolled_back=result.rolled_back,
        actions=[action_to_dict(action) for action in result.actions],
    )
    return result


def restore_original(
    conn: sqlite3.Connection, driver: Driver, snapshot: HardwareSnapshot
) -> tuple[ChangePlan, ApplyResult | None]:
    """Return the node to its stored original state, then retire that record.

    The record is retired only when the host is verifiably back at the original
    values: either the driver wrote and verified them without error, or the
    snapshot already matched. A dry-run or failed restore keeps the record.
    """
    original = load_original_state(conn, snapshot.node)
    plan = plan_restore(snapshot, original)
    result = execute_plan(conn, driver, plan, operation="restore")
    restored = result is not None and driver.writes_host and not result.error and (
        result.applied or not plan.actions
    )
    if restored:
        retire_original_state(conn, snapshot.node)
        record_audit(conn, "original_state_retired", node=snapshot.node)
    return plan, result
