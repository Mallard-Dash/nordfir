from dataclasses import replace

from nordfir.db import initiate_db
from nordfir.engine import desired_state, run_once
from nordfir.hardware import collect_hardware
from nordfir.model import Intent, PowerMode


def test_economize_targets_rest_ceiling(sysfs, procfs):
    snap = collect_hardware(sysfs("full"), procfs, node="test")
    desired = desired_state(snap, Intent.ECONOMIZE)

    assert not desired.blocked
    assert desired.mode is PowerMode.REST
    assert desired.governor == "powersave"
    assert desired.scaling_max_khz == 1_880_000  # 40% of 4.7 GHz


def test_economize_never_raises_a_lower_ceiling(sysfs, procfs):
    snap = collect_hardware(sysfs("full"), procfs, node="test")
    snap = replace(snap, cpufreq=replace(snap.cpufreq, scaling_max_khz=1_000_000))

    assert desired_state(snap, Intent.ECONOMIZE).scaling_max_khz == 1_000_000


def test_economize_blocks_on_unknown_state(sysfs, tmp_path):
    for name in ("invalid", "missing"):
        snap = collect_hardware(sysfs(name), tmp_path, node="test")
        desired = desired_state(snap, Intent.ECONOMIZE)
        assert desired.blocked
        assert desired.scaling_max_khz is None


def test_release_is_blocked_until_off_exists(sysfs, procfs):
    snap = collect_hardware(sysfs("full"), procfs, node="test")
    assert desired_state(snap, Intent.RELEASE).blocked


def test_run_once_records_snapshot_and_decision(tmp_path, sysfs, procfs):
    conn = initiate_db(tmp_path)
    run_once(conn, Intent.ECONOMIZE, sysfs_root=sysfs("full"), procfs_root=procfs, node="test")

    assert conn.execute("SELECT COUNT(*) FROM snapshots").fetchone()[0] == 1
    assert conn.execute("SELECT event FROM audit_events").fetchone()[0] == "decision"
