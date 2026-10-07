import json
import stat

from nordfir.audit import record_audit
from nordfir.db import SCHEMA_VERSION, initiate_db, save_snapshot
from nordfir.hardware import collect_hardware


def test_initiate_db_creates_private_state(tmp_path):
    state = tmp_path / "state"
    initiate_db(state).close()

    assert stat.S_IMODE(state.stat().st_mode) == 0o700
    assert stat.S_IMODE((state / "nordfir.db").stat().st_mode) == 0o600


def test_initiate_db_is_idempotent(tmp_path):
    initiate_db(tmp_path).close()
    conn = initiate_db(tmp_path)
    assert conn.execute("PRAGMA user_version").fetchone()[0] == SCHEMA_VERSION


def test_snapshot_and_audit_round_trip(tmp_path, sysfs, procfs):
    conn = initiate_db(tmp_path)
    snap = collect_hardware(sysfs("full"), procfs, node="test")

    save_snapshot(conn, snap)
    record_audit(conn, "hello", value=1)

    data = json.loads(conn.execute("SELECT data FROM snapshots").fetchone()[0])
    assert data["cpufreq"]["governor"] == "powersave"
    event, detail = conn.execute("SELECT event, detail FROM audit_events").fetchone()
    assert (event, json.loads(detail)) == ("hello", {"value": 1})
