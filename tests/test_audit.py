import json
import sqlite3

import pytest

from nordfir.audit import GENESIS, audit_head, record_audit, verify_audit
from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.driver import DryRunDriver
from nordfir.engine import execute_plan
from nordfir.plan import ChangePlan, SetScalingMax


@pytest.fixture
def conn(tmp_path):
    return initiate_db(tmp_path)


def fill(conn, n=3):
    for i in range(n):
        record_audit(conn, "event", n=i)


def drop_triggers(conn):
    conn.execute("DROP TRIGGER audit_events_no_update")
    conn.execute("DROP TRIGGER audit_events_no_delete")


def test_empty_log_verifies(conn):
    report = verify_audit(conn)
    assert report.ok and report.checked == 0 and report.head == GENESIS


def test_events_are_chained(conn):
    fill(conn)

    hashes = conn.execute("SELECT prev_hash, hash FROM audit_events ORDER BY id").fetchall()
    assert hashes[0][0] == GENESIS
    assert hashes[1][0] == hashes[0][1] and hashes[2][0] == hashes[1][1]
    report = verify_audit(conn)
    assert report.ok and report.checked == 3 and report.head == audit_head(conn) == hashes[2][1]


def test_update_and_delete_are_rejected(conn):
    fill(conn)

    with pytest.raises(sqlite3.IntegrityError, match="append-only"):
        conn.execute("UPDATE audit_events SET event = 'x'")
    with pytest.raises(sqlite3.IntegrityError, match="append-only"):
        conn.execute("DELETE FROM audit_events")
    assert verify_audit(conn).ok


def test_detects_modified_event_even_without_triggers(conn):
    fill(conn)
    drop_triggers(conn)
    conn.execute("UPDATE audit_events SET detail = ? WHERE id = 2", (json.dumps({"n": 99}),))

    report = verify_audit(conn)
    assert not report.ok and "event 2" in report.error and "modified" in report.error


def test_detects_removed_middle_event(conn):
    fill(conn, 4)
    drop_triggers(conn)
    conn.execute("DELETE FROM audit_events WHERE id = 2")

    report = verify_audit(conn)
    assert not report.ok and "event 3" in report.error and "chain broken" in report.error


def test_truncated_tail_is_not_detectable_but_head_changes(conn):
    fill(conn, 3)
    head = audit_head(conn)
    drop_triggers(conn)
    conn.execute("DELETE FROM audit_events WHERE id = 3")

    assert verify_audit(conn).ok  # the chain alone cannot see this...
    assert audit_head(conn) != head  # ...but an externally recorded head can


def test_stripped_hash_is_detected(conn):
    fill(conn)
    drop_triggers(conn)
    conn.execute("UPDATE audit_events SET hash = NULL, prev_hash = NULL WHERE id = 2")

    assert "lost its hash" in verify_audit(conn).error


def test_upgrade_from_v2_keeps_old_events_as_legacy(tmp_path):
    path = tmp_path / "nordfir.db"
    old = sqlite3.connect(path)
    old.executescript(
        """
        CREATE TABLE audit_events (
            id INTEGER PRIMARY KEY, recorded_at REAL NOT NULL,
            event TEXT NOT NULL, detail TEXT NOT NULL);
        INSERT INTO audit_events (recorded_at, event, detail) VALUES (1.0, 'old', '{}');
        PRAGMA user_version = 2;
        """
    )
    old.commit()
    old.close()

    conn = initiate_db(tmp_path)
    record_audit(conn, "new")

    report = verify_audit(conn)
    assert report.ok and report.legacy == 1 and report.checked == 1
    with pytest.raises(sqlite3.IntegrityError):
        conn.execute("DELETE FROM audit_events")  # triggers added to the old table too


def test_restore_events_are_named_distinctly(conn):
    plan = ChangePlan("test", actions=(SetScalingMax(2, 1),))
    execute_plan(conn, DryRunDriver(), plan, operation="restore")
    execute_plan(conn, DryRunDriver(), ChangePlan("test", blockers=("x",)), operation="restore")

    events = [e for e, in conn.execute("SELECT event FROM audit_events")]
    assert events == ["restore_started", "restore", "restore_refused"]


def test_cli_audit_verify(tmp_path, capsys):
    conn = initiate_db(tmp_path)
    fill(conn, 2)
    assert main(["audit-verify", "--state-dir", str(tmp_path)]) == 0
    assert '"checked": 2' in capsys.readouterr().out

    drop_triggers(conn)
    conn.execute("UPDATE audit_events SET event = 'tampered' WHERE id = 1")
    conn.commit()
    assert main(["audit-verify", "--state-dir", str(tmp_path)]) == 1
