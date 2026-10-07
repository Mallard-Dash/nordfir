"""Tamper-evident audit log.

Every event stores the hash of the previous one, so editing or removing an
event in the middle of the log breaks the chain and ``verify_audit`` says
where. SQLite triggers (see ``db.py``) also reject UPDATE and DELETE, which
stops accidents and casual edits; the chain is what catches someone who
drops the triggers.

What a chain cannot detect: events removed from the *end*. To cover that,
record ``audit_head()`` somewhere outside this machine.
"""

from __future__ import annotations

import hashlib
import json
import sqlite3
import time
from dataclasses import dataclass

GENESIS = "0" * 64


@dataclass(frozen=True)
class AuditVerification:
    ok: bool
    checked: int  # events covered by the chain
    legacy: int  # older events recorded before chaining existed (unprotected)
    head: str  # hash of the newest event, or GENESIS for an empty chain
    error: str | None = None


def _hash(prev: str, recorded_at: float, event: str, detail: str) -> str:
    payload = json.dumps([prev, recorded_at, event, detail])
    return hashlib.sha256(payload.encode()).hexdigest()


def record_audit(conn: sqlite3.Connection, event: str, **detail: object) -> None:
    """Append an event, chained to the previous one."""
    detail_text = json.dumps(detail, default=str)
    recorded_at = time.time()
    # IMMEDIATE takes the write lock first, so two writers cannot chain to the
    # same predecessor.
    conn.commit()
    conn.execute("BEGIN IMMEDIATE")
    try:
        prev = audit_head(conn)
        conn.execute(
            "INSERT INTO audit_events (recorded_at, event, detail, prev_hash, hash)"
            " VALUES (?, ?, ?, ?, ?)",
            (recorded_at, event, detail_text, prev,
             _hash(prev, recorded_at, event, detail_text)),
        )
        conn.commit()
    except BaseException:
        conn.rollback()
        raise


def audit_head(conn: sqlite3.Connection) -> str:
    row = conn.execute(
        "SELECT hash FROM audit_events WHERE hash IS NOT NULL ORDER BY id DESC LIMIT 1"
    ).fetchone()
    return row[0] if row else GENESIS


def verify_audit(conn: sqlite3.Connection) -> AuditVerification:
    """Recompute the chain from the start and report the first inconsistency."""
    prev, checked, legacy = GENESIS, 0, 0
    rows = conn.execute(
        "SELECT id, recorded_at, event, detail, prev_hash, hash FROM audit_events ORDER BY id"
    )
    for id_, recorded_at, event, detail, prev_hash, hash_ in rows:
        if hash_ is None:
            if checked:
                return AuditVerification(False, checked, legacy, prev, f"event {id_} lost its hash")
            legacy += 1
            continue
        if prev_hash != prev:
            return AuditVerification(
                False, checked, legacy, prev, f"event {id_}: chain broken (event removed or reordered)"
            )
        if hash_ != _hash(prev_hash, recorded_at, event, detail):
            return AuditVerification(False, checked, legacy, prev, f"event {id_}: content was modified")
        prev, checked = hash_, checked + 1
    return AuditVerification(True, checked, legacy, prev)
