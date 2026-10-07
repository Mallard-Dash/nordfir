from dataclasses import replace

import pytest

from nordfir.cli import main
from nordfir.engine import desired_state
from nordfir.hardware import collect_hardware
from nordfir.model import Intent
from nordfir.plan import SetGovernor, SetScalingMax, plan_changes
from nordfir.state import original_state_from


@pytest.fixture
def snap(sysfs, procfs):
    return collect_hardware(sysfs("full"), procfs, node="test")


def _plan(snapshot, intent=Intent.ECONOMIZE, original="derive"):
    if original == "derive":
        original = original_state_from(snapshot)
    return plan_changes(snapshot, desired_state(snapshot, intent), original)


def test_lowers_ceiling_only_when_governor_already_matches(snap):
    plan = _plan(snap)

    assert not plan.blocked
    assert plan.actions == (SetScalingMax(expected_khz=2_800_000, target_khz=1_880_000),)


def test_governor_is_changed_before_ceiling(snap):
    snap = replace(snap, cpufreq=replace(snap.cpufreq, governor="performance"))

    assert _plan(snap).actions == (
        SetGovernor("performance", "powersave"),
        SetScalingMax(2_800_000, 1_880_000),
    )


def test_already_at_target_has_no_actions(snap):
    snap = replace(snap, cpufreq=replace(snap.cpufreq, scaling_max_khz=1_000_000))

    plan = _plan(snap)
    assert plan.actions == () and not plan.blocked


def test_changes_are_blocked_without_original_state(snap):
    plan = _plan(snap, original=None)

    assert plan.blocked and plan.actions == ()
    assert "save-original" in plan.blockers[0]


def test_blocked_when_hardware_differs_from_original(snap):
    original = replace(original_state_from(snap), hardware_max_khz=3_000_000)

    assert _plan(snap, original=original).blocked


def test_blocked_when_ceiling_would_be_below_minimum(snap):
    snap = replace(snap, cpufreq=replace(snap.cpufreq, scaling_min_khz=2_000_000))

    assert _plan(snap).blocked


def test_desired_blockers_pass_through(sysfs, tmp_path):
    snap = collect_hardware(sysfs("missing"), tmp_path, node="test")

    plan = _plan(snap, original=None)
    assert plan.blocked and plan.blockers == desired_state(snap, Intent.ECONOMIZE).blockers


@pytest.mark.parametrize("intent", [Intent.AVAILABLE, Intent.MAINTENANCE])
def test_non_rest_intents_plan_nothing(snap, intent):
    plan = _plan(snap, intent)
    assert plan.actions == () and not plan.blocked


def test_cli_plan_changes_needs_original_state(tmp_path, sysfs, procfs, capsys):
    base = ["--sysfs-root", str(sysfs("full")), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path)]

    assert main(base + ["plan-changes", "economize", *state]) == 1
    assert main(base + ["save-original", *state]) == 0
    capsys.readouterr()
    assert main(base + ["plan-changes", "economize", *state]) == 0
    out = capsys.readouterr().out
    assert '"target_khz": 1880000' in out and "Apply: false" in out
