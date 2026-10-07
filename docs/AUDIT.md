# Audit log

Roadmap step 7. Every decision, apply, restore and refusal is recorded in the
`audit_events` table. The log is append-only and tamper-evident.

## Events

| Event                      | When                                              |
|----------------------------|---------------------------------------------------|
| `decision`                 | `plan --record` / `run_once`                      |
| `apply_started`, `apply`   | before / after a driver runs for `apply`, `dry-run` |
| `apply_refused`            | plan was blocked or deferred (blockers, deferrals) |
| `restore_started`, `restore`, `restore_refused` | same, for a restore       |
| `original_state_retired`   | after a verified restore                          |
| `observer_started`, `observation`, `observer_error`, `observer_stopped` | observer loop ([OBSERVER.md](OBSERVER.md)) |

`execute_plan(..., operation="apply" | "restore")` picks the names, so a
restore is never mistaken for an apply.

## Protection, in layers

1. **Append-only triggers.** SQLite rejects `UPDATE` and `DELETE` on
   `audit_events` (`audit events are append-only`). This stops mistakes and
   casual edits.
2. **Hash chain.** Each event stores `prev_hash` and `hash`, where
   `hash = sha256(json([prev_hash, recorded_at, event, detail]))`. Anyone who
   can drop the triggers and edit the file still breaks the chain.
   `verify_audit()` recomputes it and reports the first bad event: modified
   content, a removed or reordered event, or a stripped hash.
3. **Serialized writers.** Events are written under `BEGIN IMMEDIATE`, so two
   processes cannot chain to the same predecessor.

## What it cannot detect

- **Events removed from the end.** The remaining chain is still valid.
  Defence: record the head hash off the machine (`audit-verify` prints it) and
  compare later.
- **Someone who recomputes the whole chain.** There is no secret key; this is
  tamper *evidence* against edits, not authentication. A keyed or externally
  anchored chain would be needed for that.
- Events before the upgrade (below).

## Upgrading

Schema v3 adds the hash columns and triggers. Opening a v1/v2 database keeps
old events untouched as **legacy**: they are counted but unprotected, and the
chain starts with the first new event. Legacy rows after a hashed row count as
tampering.

## CLI

```bash
nordfir audit-verify [--state-dir DIR]
```

Prints `ok`, `checked`, `legacy`, `head` and any `error`; exit 1 if the chain
is broken.

## Not yet

- External forwarding (the Rust version used a Unix datagram collector with
  fail-closed behaviour). Roadmap step 7b.
- Keyed (HMAC) chaining.

Code: `src/nordfir/audit.py`, triggers and migration in `db.py`; tests:
`tests/test_audit.py`.
