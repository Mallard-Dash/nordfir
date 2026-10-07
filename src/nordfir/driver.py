"""Drivers: the only components that may act on a host.

The engine hands a driver a finished, unblocked plan. A driver reports what it
did; it never decides. ``DryRunDriver`` changes nothing and is the default for
every command until the Linux driver exists (roadmap step 4).
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

from nordfir.plan import Action, ChangePlan, SetGovernor, SetScalingMax


@dataclass(frozen=True)
class ApplyResult:
    driver: str
    applied: bool  # True if the host may differ from before (even after a failed rollback)
    actions: tuple[Action, ...] = ()
    error: str | None = None
    rolled_back: bool = False


class Driver(Protocol):
    name: str
    writes_host: bool  # False for drivers that never change the host

    def apply(self, plan: ChangePlan) -> ApplyResult: ...


class DryRunDriver:
    """Reports what it would do and leaves the host untouched."""

    name = "dry-run"
    writes_host = False

    def apply(self, plan: ChangePlan) -> ApplyResult:
        if plan.blocked:
            raise ValueError("refusing to apply a blocked plan")
        return ApplyResult(self.name, applied=False, actions=plan.actions)


class LinuxCpufreqDriver:
    """Applies governor and frequency-ceiling changes through Linux cpufreq.

    Safety properties, in order:

    1. Constructed only with ``confirmed=True`` (the CLI's explicit flag).
    2. Every action's ``expected`` value is checked on every CPU before the
       first write; any drift refuses the whole plan untouched.
    3. Only existing cpufreq files are written; nothing is created.
    4. Every write is read back and must match.
    5. On any failure, values already changed are written back in reverse
       order (best effort) and the result says whether that worked.
    """

    name = "linux-cpufreq"
    writes_host = True

    def __init__(self, sysfs_root: Path | str = "/sys", *, confirmed: bool = False):
        if not confirmed:
            raise ValueError("system power writes must be explicitly confirmed")
        self._root = Path(sysfs_root) / "devices/system/cpu"

    def apply(self, plan: ChangePlan) -> ApplyResult:
        if plan.blocked:
            raise ValueError("refusing to apply a blocked plan")
        if not plan.actions:
            return ApplyResult(self.name, False)  # already at target
        steps = [self._step(action) for action in plan.actions]  # raises on unknown type
        policies = sorted(
            p for p in self._root.glob("cpu[0-9]*/cpufreq") if p.is_dir()
        )
        if not policies:
            return ApplyResult(self.name, False, error="no cpufreq policies found")

        for filename, expected, _ in steps:
            for policy in policies:
                found = _read(policy / filename)
                if found != expected:
                    return ApplyResult(
                        self.name, False,
                        error=f"drift: {policy / filename} is {found!r}, expected {expected!r}",
                    )

        done: list[tuple[Path, str]] = []  # (file, previous value)
        try:
            for filename, expected, target in steps:
                for policy in policies:
                    path = policy / filename
                    self._write(path, target)
                    done.append((path, expected))
                    if _read(path) != target:
                        raise OSError(f"{path} did not take value {target!r}")
        except OSError as error:
            return self._roll_back(plan, done, str(error))
        return ApplyResult(self.name, True, plan.actions)

    def _roll_back(
        self, plan: ChangePlan, done: list[tuple[Path, str]], error: str
    ) -> ApplyResult:
        failed = []
        for path, previous in reversed(done):
            try:
                self._write(path, previous)
                if _read(path) != previous:
                    failed.append(str(path))
            except OSError:
                failed.append(str(path))
        if failed:
            error += "; rollback incomplete: " + ", ".join(failed)
        return ApplyResult(
            self.name, applied=bool(failed), actions=plan.actions, error=error,
            rolled_back=not failed,
        )

    @staticmethod
    def _step(action: Action) -> tuple[str, str, str]:
        """(file, expected, target) for an action; unknown types are refused."""
        if isinstance(action, SetGovernor):
            return "scaling_governor", action.expected, action.target
        if isinstance(action, SetScalingMax):
            return "scaling_max_freq", str(action.expected_khz), str(action.target_khz)
        raise ValueError(f"unsupported action: {action!r}")

    def _write(self, path: Path, value: str) -> None:
        if not path.is_file():
            raise OSError(f"{path} does not exist")
        path.write_text(value)


def _read(path: Path) -> str | None:
    try:
        return path.read_text().strip()
    except (OSError, UnicodeDecodeError):
        return None
