import shutil
from dataclasses import replace

import pytest

from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.engine import desired_state
from nordfir.guards import Activity, collect_activity, guard_plan
from nordfir.hardware import collect_hardware
from nordfir.model import Intent
from nordfir.plan import ChangePlan, plan_changes
from nordfir.state import original_state_from

HEADER = "  sl  local_address rem_address   st tx_queue\n"
SSH_ESTABLISHED = "   0: 0100007F:0016 0100007F:D431 01 00000000:00000000\n"
SSH_LISTEN = "   1: 00000000:0016 00000000:0000 0A 00000000:00000000\n"
OTHER_PORT = "   2: 0100007F:1F90 0100007F:D432 01 00000000:00000000\n"
NOW = 1_000_000.0


@pytest.fixture
def snap(sysfs, procfs):
    return replace(collect_hardware(sysfs("full"), procfs, node="test"), captured_at=NOW)


@pytest.fixture
def plan(snap):
    return plan_changes(snap, desired_state(snap, Intent.ECONOMIZE), original_state_from(snap))


def idle():
    return Activity(ssh_sessions=0, protected_running=())


def proc_with(tmp_path, tcp=None, tcp6=None, processes=None):
    root = tmp_path / "proc"
    (root / "net").mkdir(parents=True)
    if tcp is not None:
        (root / "net/tcp").write_text(HEADER + tcp)
    if tcp6 is not None:
        (root / "net/tcp6").write_text(HEADER + tcp6)
    for pid, comm in (processes or {}).items():
        (root / str(pid)).mkdir()
        (root / str(pid) / "comm").write_text(comm + "\n")
    return root


# --- collection -------------------------------------------------------------

def test_counts_only_established_ssh_connections(tmp_path):
    root = proc_with(tmp_path, tcp=SSH_ESTABLISHED + SSH_LISTEN + OTHER_PORT, tcp6="")
    assert collect_activity(root).ssh_sessions == 1


def test_ssh_unknown_when_no_tcp_table_is_readable(tmp_path):
    assert collect_activity(proc_with(tmp_path)).ssh_sessions is None


def test_ssh_known_with_only_tcp6_and_custom_port(tmp_path):
    root = proc_with(tmp_path, tcp6="   0: 0000:0000:0000:0000:0000:0000:0000:0001:08AE x 01 y\n")
    assert collect_activity(root, ssh_port=2222).ssh_sessions == 1


def test_finds_protected_processes_including_long_names(tmp_path):
    root = proc_with(tmp_path, tcp="", processes={1: "systemd", 7: "postgres", 9: "a-very-long-nam"})
    activity = collect_activity(root, protected=("postgres", "a-very-long-name-indeed", "nginx"))
    assert activity.protected_running == ("a-very-long-name-indeed", "postgres")


def test_protected_unknown_when_proc_is_unreadable(tmp_path):
    assert collect_activity(tmp_path / "missing", protected=("x",)).protected_running is None


def test_no_protected_list_means_nothing_to_check(tmp_path):
    assert collect_activity(proc_with(tmp_path, tcp="")).protected_running == ()


# --- guarding ---------------------------------------------------------------

def test_idle_fresh_node_passes(snap, plan):
    assert guard_plan(plan, snap, idle(), now=NOW + 1) is plan


def test_ssh_session_defers(snap, plan):
    guarded = guard_plan(plan, snap, Activity(2, ()), now=NOW)

    assert guarded.blocked and guarded.actions == () and not guarded.blockers
    assert guarded.deferrals == ("2 SSH session(s) active",)


def test_protected_process_defers(snap, plan):
    guarded = guard_plan(plan, snap, Activity(0, ("postgres",)), now=NOW)
    assert guarded.deferrals == ("protected process 'postgres' is running",)


def test_unknown_activity_blocks(snap, plan):
    guarded = guard_plan(plan, snap, Activity(None, None), now=NOW)

    assert len(guarded.blockers) == 2 and guarded.actions == ()


def test_stale_and_future_snapshots_block(snap, plan):
    assert "stale" in guard_plan(plan, snap, idle(), now=NOW + 120).blockers[0]
    assert "future" in guard_plan(plan, snap, idle(), now=NOW - 60).blockers[0]
    assert not guard_plan(plan, snap, idle(), now=NOW - 2).blocked  # small clock skew ok


def test_empty_or_already_blocked_plans_are_untouched(snap):
    empty = ChangePlan("test")
    blocked = ChangePlan("test", blockers=("x",))
    busy = Activity(5, ("x",))

    assert guard_plan(empty, snap, busy, now=NOW) is empty
    assert guard_plan(blocked, snap, busy, now=NOW) is blocked


# --- CLI ----------------------------------------------------------------------

@pytest.fixture
def cli_host(tmp_path, sysfs, procfs):
    root = tmp_path / "sys"
    shutil.copytree(sysfs("full"), root)
    proc = tmp_path / "proc"
    shutil.copytree(procfs, proc)
    return root, proc


def run(host, args, tmp_path):
    root, proc = host
    base = ["--sysfs-root", str(root), "--procfs-root", str(proc)]
    return main(base + args + ["--state-dir", str(tmp_path / "state")])


def test_cli_apply_is_deferred_by_ssh_then_proceeds(tmp_path, cli_host, capsys):
    root, proc = cli_host
    assert run(cli_host, ["save-original"], tmp_path) == 0
    (proc / "net/tcp").write_text(HEADER + SSH_ESTABLISHED)
    confirm = ["apply", "economize", "--confirm-system-power-write"]

    assert run(cli_host, confirm, tmp_path) == 1
    assert "SSH session" in capsys.readouterr().out
    cap = root / "devices/system/cpu/cpu0/cpufreq/scaling_max_freq"
    assert cap.read_text().strip() == "2800000"

    (proc / "net/tcp").write_text(HEADER)
    assert run(cli_host, confirm, tmp_path) == 0
    assert cap.read_text().strip() == "1880000"


def test_cli_protect_defers_and_restore_is_never_guarded(tmp_path, cli_host):
    root, proc = cli_host
    run(cli_host, ["save-original"], tmp_path)
    (proc / "42").mkdir()
    (proc / "42/comm").write_text("postgres\n")

    assert run(cli_host, ["dry-run", "economize", "--protect", "postgres"], tmp_path) == 1
    assert run(cli_host, ["dry-run", "economize"], tmp_path) == 0  # not protected

    run(cli_host, ["apply", "economize", "--confirm-system-power-write"], tmp_path)
    (proc / "net/tcp").write_text(HEADER + SSH_ESTABLISHED)
    assert run(cli_host, ["restore", "--confirm-system-power-write"], tmp_path) == 0
    events = [e for e, in initiate_db(tmp_path / "state").execute("SELECT event FROM audit_events")]
    assert "original_state_retired" in events
