"""Local SQLite store for snapshots and audit events.

The state directory is private (0700) and the database file is 0600.
"""

from __future__ import annotations

import dataclasses
import json
import os
import sqlite3
from pathlib import Path

from nordfir.model import HardwareSnapshot

SCHEMA_VERSION = 4

_SCHEMA = """
CREATE TABLE IF NOT EXISTS snapshots (
    id          INTEGER PRIMARY KEY,
    node        TEXT    NOT NULL,
    captured_at REAL    NOT NULL,
    data        TEXT    NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_events (
    id          INTEGER PRIMARY KEY,
    recorded_at REAL    NOT NULL,
    event       TEXT    NOT NULL,
    detail      TEXT    NOT NULL,
    prev_hash   TEXT,
    hash        TEXT
);
-- Append-only: events can be added, never changed or removed.
CREATE TRIGGER IF NOT EXISTS audit_events_no_update
    BEFORE UPDATE ON audit_events
    BEGIN SELECT RAISE(ABORT, 'audit events are append-only'); END;
CREATE TRIGGER IF NOT EXISTS audit_events_no_delete
    BEFORE DELETE ON audit_events
    BEGIN SELECT RAISE(ABORT, 'audit events are append-only'); END;
CREATE TABLE IF NOT EXISTS power_models (
    id          INTEGER PRIMARY KEY,
    node        TEXT    NOT NULL,
    created_at  REAL    NOT NULL,
    data        TEXT    NOT NULL
);
CREATE TABLE IF NOT EXISTS original_states (
    id          INTEGER PRIMARY KEY,
    node        TEXT    NOT NULL,
    captured_at REAL    NOT NULL,
    data        TEXT    NOT NULL,
    retired_at  REAL
);
-- At most one active (not retired) recovery state per node.
CREATE UNIQUE INDEX IF NOT EXISTS one_active_original_state
    ON original_states (node) WHERE retired_at IS NULL;
"""


def initiate_db(state_dir: Path | str) -> sqlite3.Connection:
    """Create (or open) the database and make sure the schema is current."""
    state_dir = Path(state_dir)
    state_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    path = state_dir / "nordfir.db"
    if not path.exists():
        # Create the file with private permissions before SQLite touches it.
        os.close(os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600))

    conn = sqlite3.connect(path)
    try:
        version = conn.execute("PRAGMA user_version").fetchone()[0]
        if version > SCHEMA_VERSION:
            raise RuntimeError(
                f"database schema v{version} is newer than supported v{SCHEMA_VERSION}"
            )
        conn.executescript(_SCHEMA)
        _add_missing_audit_columns(conn)
        conn.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        conn.commit()
    except BaseException:
        conn.close()
        raise
    return conn


def open_existing_db(state_dir: Path | str) -> sqlite3.Connection | None:
    """Open the database if there is one; never create the state directory.

    For commands that only look at stored state: running them on a fresh host
    must not leave files behind.
    """
    if not (Path(state_dir) / "nordfir.db").is_file():
        return None
    return initiate_db(state_dir)


def _add_missing_audit_columns(conn: sqlite3.Connection) -> None:
    """Upgrade a v1/v2 database. Old events stay as they are, without hashes."""
    columns = {row[1] for row in conn.execute("PRAGMA table_info(audit_events)")}
    for name in ("prev_hash", "hash"):
        if name not in columns:
            conn.execute(f"ALTER TABLE audit_events ADD COLUMN {name} TEXT")


def save_snapshot(conn: sqlite3.Connection, snapshot: HardwareSnapshot) -> int:
    data = json.dumps(dataclasses.asdict(snapshot))
    with conn:
        cursor = conn.execute(
            "INSERT INTO snapshots (node, captured_at, data) VALUES (?, ?, ?)",
            (snapshot.node, snapshot.captured_at, data),
        )
    return cursor.lastrowid



def prune_snapshots(conn: sqlite3.Connection, node: str, keep: int) -> int:
    """Delete all but the newest ``keep`` snapshots of ``node``; returns how many.

    Audit events refer to snapshot ids, so pruned ids simply no longer resolve.
    """
    if keep < 1:
        raise ValueError("keep must be at least 1")
    with conn:
        cursor = conn.execute(
            "DELETE FROM snapshots WHERE node = ? AND id NOT IN ("
            " SELECT id FROM snapshots WHERE node = ? ORDER BY id DESC LIMIT ?)",
            (node, node, keep),
        )
    return cursor.rowcount
