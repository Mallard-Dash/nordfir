"""Safety guards: reasons not to economize *right now*.

Two kinds of reason, both stopping the plan:

- **blockers**: we cannot tell whether it is safe (unknown is not safe), or
  the input is not trustworthy (stale or future-dated snapshot);
- **deferrals**: we can tell, and the node is busy (SSH session, protected
  process running). Try again later.

Guards apply to REST changes only. Restore is never guarded: recovery must
always be possible.
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from pathlib import Path

from nordfir.model import HardwareSnapshot
from nordfir.plan import ChangePlan

MAX_SNAPSHOT_AGE_SECONDS = 60.0
CLOCK_SKEW_SECONDS = 5.0
_TCP_ESTABLISHED = "01"


@dataclass(frozen=True)
class Activity:
    """What the node is doing. ``None`` means it could not be determined."""

    ssh_sessions: int | None
    protected_running: tuple[str, ...] | None  # None: process list unreadable


def collect_activity(
    procfs_root: Path | str = "/proc",
    protected: tuple[str, ...] = (),
    ssh_port: int = 22,
) -> Activity:
    """Read-only: counts established SSH connections and protected processes."""
    procfs = Path(procfs_root)
    return Activity(
        ssh_sessions=_ssh_sessions(procfs, ssh_port),
        protected_running=_running(procfs, protected) if protected else (),
    )


def _ssh_sessions(procfs: Path, port: int) -> int | None:
    count, readable = 0, False
    for name in ("tcp", "tcp6"):
        try:
            lines = (procfs / "net" / name).read_text().splitlines()[1:]
        except (OSError, UnicodeDecodeError):
            continue
        readable = True
        for line in lines:
            fields = line.split()
            if len(fields) > 3 and fields[3] == _TCP_ESTABLISHED:
                if fields[1].rpartition(":")[2].upper() == f"{port:04X}":
                    count += 1
    # tcp6 may legitimately be absent, but one of them must be readable.
    return count if readable else None


def _running(procfs: Path, protected: tuple[str, ...]) -> tuple[str, ...] | None:
    wanted = {name[:15]: name for name in protected}  # comm is truncated to 15
    found: set[str] = set()
    try:
        entries = [e for e in procfs.iterdir() if e.name.isdigit()]
    except OSError:
        return None
    for entry in entries:
        try:
            comm = (entry / "comm").read_text().strip()
        except (OSError, UnicodeDecodeError):
            continue  # the process may have exited meanwhile
        if comm in wanted:
            found.add(wanted[comm])
    return tuple(sorted(found))


def guard_plan(
    plan: ChangePlan,
    snapshot: HardwareSnapshot,
    activity: Activity,
    *,
    now: float | None = None,
    max_age: float = MAX_SNAPSHOT_AGE_SECONDS,
) -> ChangePlan:
    """Return ``plan`` unchanged if it is safe to act, else a plan with no actions."""
    if plan.blocked or not plan.actions:
        return plan
    now = time.time() if now is None else now

    blockers = []
    age = now - snapshot.captured_at
    if age < -CLOCK_SKEW_SECONDS:
        blockers.append("snapshot is dated in the future")
    elif age > max_age:
        blockers.append(f"snapshot is stale ({age:.0f}s old, limit {max_age:.0f}s)")
    if activity.ssh_sessions is None:
        blockers.append("SSH session state is unknown")
    if activity.protected_running is None:
        blockers.append("protected process state is unknown")

    deferrals = []
    if activity.ssh_sessions:
        deferrals.append(f"{activity.ssh_sessions} SSH session(s) active")
    for name in activity.protected_running or ():
        deferrals.append(f"protected process {name!r} is running")

    if not blockers and not deferrals:
        return plan
    return ChangePlan(plan.node, (), tuple(blockers), tuple(deferrals))
