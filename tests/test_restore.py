import shutil
from dataclasses import replace

import pytest

from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.driver import DryRunDriver, LinuxCpufreqDriver
from nordfir.engine import desired_state, execute_plan, restore_original
from nordfir.hardware import collect_hardware
from nordfir.model import Intent
from nordfir.plan import SetGovernor, SetScalingMax, plan_changes, plan_restore
from nordfir.state import load_original_state, save_original_state


@pytest.fixture
def host(tmp_path, sysfs):
    root = tmp_path / "sys"
    shutil.copytree(sysfs("full"), root)
    cpu = root / "devices/system/cpu"
    shutil.copytree(cpu / "cpu0", cpu / "cpu1")
    return root


@pytest.fixture
def conn(tmp_path):
    return initiate_db(tmp_path / "state")


def economized(conn, host, procfs):
    """Save original state, then apply economize for real; return the driver."""
    snap = collect_hardware(host, procfs, node="test")
    original = save_original_state(conn, snap)
    driver = LinuxCpufreqDriver(host, confirmed=True)
    execute_plan(conn, driver, plan_changes(snap, desired_state(snap, Intent.ECONOMIZE), original))
    return driver


def current(host, procfs):
    return collect_hardware(host, procfs, node="test")


def test_plan_restore_reverses_the_change(conn, host, procfs):
    economized(conn, host, procfs)
    plan = plan_restore(current(host, procfs), load_original_state(conn, "test"))

    assert plan.actions == (SetScalingMax(1_880_000, 2_800_000),)


def test_plan_restore_includes_governor_when_changed(conn, host, procfs):
    original = save_original_state(conn, current(host, procfs))
    snap = current(host, procfs)
    snap = replace(snap, cpufreq=replace(snap.cpufreq, governor="performance"))

    assert plan_restore(snap, original).actions[0] == SetGovernor("performance", "powersave")


def test_plan_restore_blocks_without_state_or_on_hardware_change(conn, host, procfs):
    snap = current(host, procfs)
    assert plan_restore(snap, None).blocked

    original = replace(save_original_state(conn, snap), hardware_max_khz=3_000_000)
    assert plan_restore(snap, original).blocked
    assert plan_restore(snap, replace(original, node="other")).blocked


def test_restore_returns_host_and_retires_state(conn, host, procfs):
    driver = economized(conn, host, procfs)

    plan, result = restore_original(conn, driver, current(host, procfs))

    assert result.applied and not result.error
    assert current(host, procfs).cpufreq.scaling_max_khz == 2_800_000
    assert load_original_state(conn, "test") is None
    events = [e for e, in conn.execute("SELECT event FROM audit_events")]
    assert events[-1] == "original_state_retired"
    # A new original state can be saved afterwards.
    save_original_state(conn, current(host, procfs))


def test_restore_when_already_original_just_retires(conn, host, procfs):
    save_original_state(conn, current(host, procfs))
    driver = LinuxCpufreqDriver(host, confirmed=True)

    plan, result = restore_original(conn, driver, current(host, procfs))

    assert plan.actions == () and not result.applied
    assert load_original_state(conn, "test") is None


def test_dry_run_restore_keeps_state(conn, host, procfs):
    economized(conn, host, procfs)

    restore_original(conn, DryRunDriver(), current(host, procfs))

    assert load_original_state(conn, "test") is not None
    assert current(host, procfs).cpufreq.scaling_max_khz == 1_880_000


def test_failed_restore_keeps_state(conn, host, procfs):
    economized(conn, host, procfs)

    class Fails(LinuxCpufreqDriver):
        def _write(self, path, value):
            raise OSError("boom")

    _, result = restore_original(conn, Fails(host, confirmed=True), current(host, procfs))

    assert result.error
    assert load_original_state(conn, "test") is not None


def test_restore_without_original_state_is_refused(conn, host, procfs):
    plan, result = restore_original(conn, LinuxCpufreqDriver(host, confirmed=True),
                                    current(host, procfs))
    assert plan.blocked and result is None


def test_cli_apply_then_restore(tmp_path, host, procfs, capsys):
    base = ["--sysfs-root", str(host), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path / "cli-state")]
    confirm = "--confirm-system-power-write"
    assert main(base + ["save-original", *state]) == 0
    assert main(base + ["apply", "economize", confirm, *state]) == 0

    assert main(base + ["plan-restore", *state]) == 0
    assert main(base + ["restore", *state]) == 1  # refused without the flag
    assert current(host, procfs).cpufreq.scaling_max_khz == 1_880_000

    assert main(base + ["restore", confirm, *state]) == 0
    assert current(host, procfs).cpufreq.scaling_max_khz == 2_800_000
    assert main(base + ["plan-restore", *state]) == 1  # state retired
