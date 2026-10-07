import json
import os
import signal
import threading
import time

import pytest

from nordfir.audit import verify_audit
from nordfir.cli import main
from nordfir.db import initiate_db, prune_snapshots, save_snapshot
from nordfir.hardware import collect_hardware
from nordfir.observer import observe


@pytest.fixture
def conn(tmp_path):
    return initiate_db(tmp_path / "state")


def events(conn):
    return [(e, json.loads(d)) for e, d in conn.execute("SELECT event, detail FROM audit_events")]


def count(conn, table):
    return conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]


def test_bounded_run_records_snapshots_and_observations(conn, sysfs, procfs):
    summary = observe(conn, interval=0.01, stop=threading.Event(), iterations=3,
                      sysfs_root=sysfs("full"), procfs_root=procfs, node="test")

    assert (summary.cycles, summary.errors, summary.reason) == (3, 0, "iterations")
    assert count(conn, "snapshots") == 3
    names = [e for e, _ in events(conn)]
    assert names[0] == "observer_started" and names[-1] == "observer_stopped"
    assert names.count("observation") == 3
    first = events(conn)[1][1]
    assert first["governor"] == "powersave" and first["ssh_sessions"] == 0
    assert verify_audit(conn).ok


def test_observer_never_changes_the_host(conn, sysfs, procfs, tmp_path):
    import shutil
    root = tmp_path / "sys"
    shutil.copytree(sysfs("full"), root)
    before = {p: p.read_text() for p in root.rglob("*") if p.is_file()}

    observe(conn, interval=0.01, stop=threading.Event(), iterations=2,
            sysfs_root=root, procfs_root=procfs)

    assert before == {p: p.read_text() for p in root.rglob("*") if p.is_file()}


def test_stop_event_ends_the_loop_promptly(conn, sysfs, procfs):
    stop = threading.Event()
    threading.Timer(0.2, stop.set).start()

    started = time.monotonic()
    summary = observe(conn, interval=30, stop=stop, sysfs_root=sysfs("full"), procfs_root=procfs)

    assert summary.reason == "stopped" and time.monotonic() - started < 5
    assert events(conn)[-1][1]["reason"] == "stopped"


def test_already_stopped_does_nothing_but_log(conn, sysfs, procfs):
    stop = threading.Event()
    stop.set()
    summary = observe(conn, interval=1, stop=stop, sysfs_root=sysfs("full"), procfs_root=procfs)
    assert summary.cycles == 0 and count(conn, "snapshots") == 0


def test_errors_are_audited_and_bounded(conn, procfs, monkeypatch):
    def boom(**_):
        raise OSError("sensor gone")

    monkeypatch.setattr("nordfir.observer.collect_hardware", boom)
    summary = observe(conn, interval=0.001, stop=threading.Event(), iterations=100,
                      max_consecutive_errors=3, procfs_root=procfs)

    assert (summary.errors, summary.reason) == (3, "too many errors")
    errors = [d for e, d in events(conn) if e == "observer_error"]
    assert len(errors) == 3 and "sensor gone" in errors[0]["error"]


def test_a_single_failure_does_not_stop_the_observer(conn, sysfs, procfs, monkeypatch):
    real, calls = collect_hardware, []

    def flaky(**kwargs):
        calls.append(1)
        if len(calls) == 2:
            raise OSError("blip")
        return real(**kwargs)

    monkeypatch.setattr("nordfir.observer.collect_hardware", flaky)
    summary = observe(conn, interval=0.001, stop=threading.Event(), iterations=4,
                      sysfs_root=sysfs("full"), procfs_root=procfs)

    assert (summary.cycles, summary.errors, summary.reason) == (4, 1, "iterations")


def test_snapshot_retention(conn, sysfs, procfs):
    observe(conn, interval=0.001, stop=threading.Event(), iterations=5, keep_snapshots=2,
            sysfs_root=sysfs("full"), procfs_root=procfs, node="test")
    assert count(conn, "snapshots") == 2


def test_prune_snapshots_is_per_node(conn, sysfs, procfs):
    for node in ("a", "a", "a", "b"):
        save_snapshot(conn, collect_hardware(sysfs("full"), procfs, node=node))

    assert prune_snapshots(conn, "a", 1) == 2
    assert count(conn, "snapshots") == 2
    with pytest.raises(ValueError):
        prune_snapshots(conn, "a", 0)


def test_rejects_bad_interval(conn):
    with pytest.raises(ValueError):
        observe(conn, interval=0, stop=threading.Event())


def test_cli_bounded_run(tmp_path, sysfs, procfs, capsys):
    base = ["--sysfs-root", str(sysfs("full")), "--procfs-root", str(procfs)]
    code = main(base + ["observe", "--interval", "0.01", "--iterations", "2",
                        "--state-dir", str(tmp_path)])

    assert code == 0 and '"cycles": 2' in capsys.readouterr().out
    assert main(["observe", "--interval", "0", "--state-dir", str(tmp_path)]) == 1


def test_cli_stops_cleanly_on_sigterm_and_restores_handlers(tmp_path, sysfs, procfs, capsys):
    before = signal.getsignal(signal.SIGTERM)
    base = ["--sysfs-root", str(sysfs("full")), "--procfs-root", str(procfs)]
    threading.Timer(0.3, lambda: os.kill(os.getpid(), signal.SIGTERM)).start()

    started = time.monotonic()
    code = main(base + ["observe", "--interval", "30", "--state-dir", str(tmp_path)])

    assert code == 0 and time.monotonic() - started < 10
    assert '"reason": "stopped"' in capsys.readouterr().out
    assert signal.getsignal(signal.SIGTERM) is before
