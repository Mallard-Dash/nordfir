"""Typed change planning: what would have to change to reach a desired state.

A plan is a value. Building one writes nothing; applying it is a driver's job
(roadmap steps 3-4). Every action carries the value it expects to find, so a
driver can refuse to act if the host has drifted since the plan was made.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field

from nordfir.model import DesiredState, HardwareSnapshot, PowerMode
from nordfir.state import OriginalState


@dataclass(frozen=True)
class SetGovernor:
    expected: str
    target: str


@dataclass(frozen=True)
class SetScalingMax:
    expected_khz: int
    target_khz: int


Action = SetGovernor | SetScalingMax


@dataclass(frozen=True)
class ChangePlan:
    node: str
    actions: tuple[Action, ...] = ()
    blockers: tuple[str, ...] = field(default_factory=tuple)
    # Reasons to wait: the node is busy, not broken. Retry later.
    deferrals: tuple[str, ...] = field(default_factory=tuple)

    @property
    def blocked(self) -> bool:
        return bool(self.blockers or self.deferrals)


def action_to_dict(action: Action) -> dict:
    """JSON-friendly form of an action, tagged with its type."""
    return {"type": type(action).__name__, **asdict(action)}


def plan_changes(
    snapshot: HardwareSnapshot,
    desired: DesiredState,
    original: OriginalState | None,
) -> ChangePlan:
    """Plan the actions that move ``snapshot`` to ``desired``.

    Actions are ordered governor first, then frequency ceiling. A plan with no
    actions means the node is already there (or the intent changes nothing).
    """
    node = snapshot.node
    if desired.blocked:
        return ChangePlan(node, blockers=desired.blockers)
    if desired.mode is not PowerMode.REST:
        return ChangePlan(node)  # ACTIVE restore is step 5; maintenance is a no-op

    cpu = snapshot.cpufreq
    actions: list[Action] = []
    if desired.governor != cpu.governor:
        actions.append(SetGovernor(cpu.governor, desired.governor))
    if desired.scaling_max_khz != cpu.scaling_max_khz:
        actions.append(SetScalingMax(cpu.scaling_max_khz, desired.scaling_max_khz))
    if not actions:
        return ChangePlan(node)

    blockers = _blockers(snapshot, desired, original)
    return ChangePlan(node, () if blockers else tuple(actions), tuple(blockers))


def _blockers(
    snapshot: HardwareSnapshot, desired: DesiredState, original: OriginalState | None
) -> list[str]:
    cpu = snapshot.cpufreq
    blockers = []
    if original is None:
        blockers.append("no original state is stored; run save-original first")
    elif original.node != snapshot.node:
        blockers.append("original state belongs to a different node")
    elif (original.hardware_min_khz, original.hardware_max_khz) != (
        cpu.hardware_min_khz,
        cpu.hardware_max_khz,
    ):
        blockers.append("hardware frequency range differs from the stored original state")
    if cpu.scaling_min_khz is None:
        blockers.append("current minimum frequency is unknown")
    elif desired.scaling_max_khz < cpu.scaling_min_khz:
        blockers.append("target ceiling is below the current minimum frequency")
    return blockers


def plan_restore(snapshot: HardwareSnapshot, original: OriginalState | None) -> ChangePlan:
    """Plan the actions that return ``snapshot`` to the stored original state.

    Validated against the hardware as it is now: a changed frequency range or a
    different node blocks the restore rather than writing stale values.
    """
    node = snapshot.node
    cpu = snapshot.cpufreq
    if original is None:
        return ChangePlan(node, blockers=("no original state is stored",))

    blockers = []
    if original.node != node:
        blockers.append("original state belongs to a different node")
    if not snapshot.cpufreq_available:
        blockers.append("cpufreq interface is not available")
    if cpu.governor is None:
        blockers.append("current governor is unknown")
    if cpu.scaling_max_khz is None:
        blockers.append("current maximum frequency is unknown")
    if (original.hardware_min_khz, original.hardware_max_khz) != (
        cpu.hardware_min_khz,
        cpu.hardware_max_khz,
    ):
        blockers.append("hardware frequency range differs from the stored original state")
    if cpu.scaling_min_khz is not None and original.scaling_max_khz < cpu.scaling_min_khz:
        blockers.append("original ceiling is below the current minimum frequency")
    if blockers:
        return ChangePlan(node, blockers=tuple(blockers))

    actions: list[Action] = []
    if cpu.governor != original.governor:
        actions.append(SetGovernor(cpu.governor, original.governor))
    if cpu.scaling_max_khz != original.scaling_max_khz:
        actions.append(SetScalingMax(cpu.scaling_max_khz, original.scaling_max_khz))
    return ChangePlan(node, tuple(actions))
