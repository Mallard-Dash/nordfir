import json
import shutil
from dataclasses import replace

import pytest

from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.driver import LinuxCpufreqDriver
from nordfir.engine import desired_state, execute_plan
from nordfir.hardware import collect_hardware
from nordfir.model import Intent
from nordfir.plan import ChangePlan, SetGovernor, plan_changes
from nordfir.state import original_state_from


@pytest.fixture
def host(tmp_path, sysfs):
    """A writable copy of the fixture sysfs with two CPUs."""
    root = tmp_path / "sys"
    shutil.copytree(sysfs("full"), root)
    cpu = root / "devices/system/cpu"
    shutil.copytree(cpu / "cpu0", cpu / "cpu1")
    return root


def value(host, cpu, name):
    return (host / f"devices/system/cpu/cpu{cpu}/cpufreq/{name}").read_text().strip()


def make_plan(host, procfs, governor=None):
    snap = collect_hardware(host, procfs, node="test")
    if governor:
        snap = replace(snap, cpufreq=replace(snap.cpufreq, governor=governor))
    return plan_changes(snap, desired_state(snap, Intent.ECONOMIZE), original_state_from(snap))


def test_requires_explicit_confirmation(host):
    with pytest.raises(ValueError, match="confirmed"):
        LinuxCpufreqDriver(host)


def test_applies_to_every_cpu_and_verifies(host, procfs):
    result = LinuxCpufreqDriver(host, confirmed=True).apply(make_plan(host, procfs))

    assert result.applied and result.error is None
    assert [value(host, c, "scaling_max_freq") for c in (0, 1)] == ["1880000"] * 2


def test_drift_refuses_before_any_write(host, procfs):
    plan = make_plan(host, procfs)
    (host / "devices/system/cpu/cpu1/cpufreq/scaling_max_freq").write_text("3000000")

    result = LinuxCpufreqDriver(host, confirmed=True).apply(plan)

    assert not result.applied and "drift" in result.error
    assert value(host, 0, "scaling_max_freq") == "2800000"  # untouched


def test_rolls_back_when_a_write_fails(host, procfs):
    class FailsOnCpu1(LinuxCpufreqDriver):
        def _write(self, path, value):
            if "cpu1" in str(path) and value == "1880000":
                raise OSError("boom")
            super()._write(path, value)

    result = FailsOnCpu1(host, confirmed=True).apply(make_plan(host, procfs))

    assert result.rolled_back and "boom" in result.error
    assert [value(host, c, "scaling_max_freq") for c in (0, 1)] == ["2800000"] * 2


def test_rolls_back_when_verification_fails(host, procfs):
    class IgnoresWrites(LinuxCpufreqDriver):
        def _write(self, path, value):
            if value != "2800000":  # silently drop everything but the rollback
                return
            super()._write(path, value)

    result = IgnoresWrites(host, confirmed=True).apply(make_plan(host, procfs))

    assert not result.applied and result.rolled_back and "did not take" in result.error


def test_reports_incomplete_rollback(host, procfs):
    class BrokenRollback(LinuxCpufreqDriver):
        def _write(self, path, value):
            if value == "2800000" or "cpu1" in str(path):
                raise OSError("stuck")
            super()._write(path, value)

    result = BrokenRollback(host, confirmed=True).apply(make_plan(host, procfs))

    assert result.applied and not result.rolled_back and "rollback incomplete" in result.error


def test_governor_then_ceiling(host, procfs):
    (host / "devices/system/cpu/cpu0/cpufreq/scaling_governor").write_text("performance")
    (host / "devices/system/cpu/cpu1/cpufreq/scaling_governor").write_text("performance")
    plan = make_plan(host, procfs)  # recollected: governor is performance now
    assert isinstance(plan.actions[0], SetGovernor)

    assert LinuxCpufreqDriver(host, confirmed=True).apply(plan).applied
    assert value(host, 1, "scaling_governor") == "powersave"


def test_empty_plan_changes_nothing(host):
    result = LinuxCpufreqDriver(host, confirmed=True).apply(ChangePlan("test"))
    assert not result.applied and result.error is None


def test_refuses_blocked_plan_and_unknown_actions(host):
    driver = LinuxCpufreqDriver(host, confirmed=True)
    with pytest.raises(ValueError):
        driver.apply(ChangePlan("test", blockers=("x",)))
    with pytest.raises(ValueError, match="unsupported"):
        driver.apply(ChangePlan("test", actions=("reboot",)))


def test_never_creates_files(host, procfs):
    (host / "devices/system/cpu/cpu1/cpufreq/scaling_max_freq").unlink()
    plan = make_plan(host, procfs)  # cpu0 only sees the file

    result = LinuxCpufreqDriver(host, confirmed=True).apply(plan)

    assert not result.applied and "drift" in result.error
    assert not (host / "devices/system/cpu/cpu1/cpufreq/scaling_max_freq").exists()


def test_execute_plan_audits_start_and_outcome(tmp_path, host, procfs):
    conn = initiate_db(tmp_path / "state")
    execute_plan(conn, LinuxCpufreqDriver(host, confirmed=True), make_plan(host, procfs))

    rows = [(e, json.loads(d)) for e, d in conn.execute("SELECT event, detail FROM audit_events")]
    assert [e for e, _ in rows] == ["apply_started", "apply"]
    assert rows[1][1]["applied"] is True and rows[1][1]["driver"] == "linux-cpufreq"


def test_cli_apply_requires_flag_and_writes_when_confirmed(tmp_path, host, procfs, capsys):
    base = ["--sysfs-root", str(host), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path / "state")]
    assert main(base + ["save-original", *state]) == 0

    assert main(base + ["apply", "economize", *state]) == 1
    assert value(host, 0, "scaling_max_freq") == "2800000"

    capsys.readouterr()
    assert main(base + ["apply", "economize", "--confirm-system-power-write", *state]) == 0
    assert value(host, 0, "scaling_max_freq") == "1880000"
    assert "Apply: true" in capsys.readouterr().out
