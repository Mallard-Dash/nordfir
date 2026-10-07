import os
import shutil
import sqlite3

import pytest

from nordfir.audit import record_audit
from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.hardware import collect_hardware
from nordfir.preflight import run_preflight
from nordfir.state import save_original_state

SELF_STATUS = "Name:\tnordfir\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nNoNewPrivs:\t1\nCapEff:\t0000000000000000\nSeccomp:\t2\n"
needs_non_root = pytest.mark.skipif(os.geteuid() == 0, reason="root ignores file permissions")


@pytest.fixture
def host(tmp_path, sysfs, procfs):
    sys_root, proc_root = tmp_path / "sys", tmp_path / "proc"
    shutil.copytree(sysfs("full"), sys_root)
    shutil.copytree(procfs, proc_root)
    (proc_root / "self").mkdir()
    (proc_root / "self/status").write_text(SELF_STATUS.format(uid=os.geteuid()))
    state = tmp_path / "state"
    conn = initiate_db(state)
    save_original_state(conn, collect_hardware(sys_root, proc_root))
    conn.close()
    return {"sysfs_root": sys_root, "procfs_root": proc_root, "state_dir": state}


def statuses(report):
    return {c.name: c.status for c in report.checks}


def test_ready_host_passes_everything(host):
    report = run_preflight("apply", **host)

    assert report.ready
    assert set(statuses(report).values()) == {"pass"}


def test_preflight_creates_and_changes_nothing(host, tmp_path):
    fresh = {**host, "state_dir": tmp_path / "not-yet"}
    before = {p: p.read_bytes() for p in tmp_path.rglob("*") if p.is_file()}

    report = run_preflight("apply", **fresh)

    assert not (tmp_path / "not-yet").exists()
    assert before == {p: p.read_bytes() for p in tmp_path.rglob("*") if p.is_file()}
    assert statuses(report)["state-dir"] == "pass"
    assert statuses(report)["original-state"] == "warn" and report.ready


@needs_non_root
def test_unwritable_cpufreq_fails_apply_but_not_observe(host):
    (host["sysfs_root"] / "devices/system/cpu/cpu0/cpufreq/scaling_max_freq").chmod(0o444)

    apply = run_preflight("apply", **host)
    assert not apply.ready and statuses(apply)["cpufreq-write"] == "fail"
    assert run_preflight("observe", **host).ready


def test_unknown_cpufreq_fails_apply(host, sysfs):
    host["sysfs_root"] = sysfs("invalid")

    report = run_preflight("apply", **host)
    assert not report.ready and "baseline" in next(c for c in report.checks if c.name == "cpufreq").detail


def test_ssh_visibility_is_required(host):
    (host["procfs_root"] / "net/tcp").unlink()
    assert run_preflight("observe", **host).ready  # tcp6 still readable
    (host["procfs_root"] / "net/tcp6").unlink()

    report = run_preflight("observe", **host)
    assert not report.ready and statuses(report)["ssh-visibility"] == "fail"


def test_missing_proc_files_fail(host):
    (host["procfs_root"] / "meminfo").unlink()
    assert statuses(run_preflight("observe", **host))["proc-basics"] == "fail"


@needs_non_root
def test_state_dir_and_db_must_be_private(host):
    host["state_dir"].chmod(0o755)
    report = run_preflight("observe", **host)
    detail = next(c for c in report.checks if c.name == "state-dir").detail
    assert statuses(report)["state-dir"] == "fail" and "group/others" in detail

    host["state_dir"].chmod(0o700)
    (host["state_dir"] / "nordfir.db").chmod(0o644)
    assert statuses(run_preflight("observe", **host))["state-dir"] == "fail"


def test_wrong_owner_fails(host):
    report = run_preflight("observe", uid=os.geteuid() + 1, **host)
    assert statuses(report)["state-dir"] == "fail" and "owned by uid" in next(
        c for c in report.checks if c.name == "state-dir").detail


def test_tampered_audit_chain_fails(host):
    conn = sqlite3.connect(host["state_dir"] / "nordfir.db")
    for i in range(3):
        record_audit(conn, "e", i=i)
    conn.execute("DROP TRIGGER audit_events_no_update")
    conn.execute("UPDATE audit_events SET detail = '{}' WHERE id = 2")
    conn.commit()
    conn.close()

    report = run_preflight("observe", **host)
    assert not report.ready and statuses(report)["audit-chain"] == "fail"


def test_newer_schema_fails(host):
    conn = sqlite3.connect(host["state_dir"] / "nordfir.db")
    conn.execute("PRAGMA user_version = 99")
    conn.commit()
    conn.close()

    assert statuses(run_preflight("observe", **host))["database"] == "fail"


def test_no_original_state_is_only_a_warning(host):
    conn = sqlite3.connect(host["state_dir"] / "nordfir.db")
    conn.execute("UPDATE original_states SET retired_at = 1")
    conn.commit()
    conn.close()

    report = run_preflight("apply", **host)
    assert report.ready and statuses(report)["original-state"] == "warn"


def test_pending_restore_is_reported(host):
    (host["sysfs_root"] / "devices/system/cpu/cpu0/cpufreq/scaling_max_freq").write_text("1000000")

    report = run_preflight("apply", **host)
    assert report.ready and statuses(report)["pending-restore"] == "warn"


def test_confinement_warnings(host):
    status = host["procfs_root"] / "self/status"
    status.write_text(SELF_STATUS.format(uid=0).replace("NoNewPrivs:\t1", "NoNewPrivs:\t0"))
    detail = next(c for c in run_preflight("observe", **host).checks if c.name == "confinement")
    assert detail.status == "warn" and "root" in detail.detail and "NoNewPrivs" in detail.detail

    status.write_text(SELF_STATUS.format(uid=1000).replace("0000000000000000", "0000000000003000"))
    assert "capabilities" in next(c for c in run_preflight("observe", **host).checks
                                  if c.name == "confinement").detail

    status.unlink()
    assert statuses(run_preflight("observe", **host))["confinement"] == "warn"


def test_all_problems_are_reported_together(host):
    (host["procfs_root"] / "meminfo").unlink()
    (host["procfs_root"] / "net/tcp").unlink()
    (host["procfs_root"] / "net/tcp6").unlink()

    failed = [c.name for c in run_preflight("observe", **host).checks if c.status == "fail"]
    assert failed == ["ssh-visibility", "proc-basics"]


def test_rejects_unknown_role(host):
    with pytest.raises(ValueError):
        run_preflight("admin", **host)


def test_cli_exit_code_follows_readiness(host, capsys):
    base = ["--sysfs-root", str(host["sysfs_root"]), "--procfs-root", str(host["procfs_root"]),
            "preflight", "--state-dir", str(host["state_dir"])]
    assert main(base) == 0
    assert '"ready": true' in capsys.readouterr().out

    (host["procfs_root"] / "meminfo").unlink()
    assert main(base + ["--role", "observe"]) == 1
