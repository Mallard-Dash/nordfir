# Observer loop

Roadmap step 8. A long-running, **read-only** loop that records how the node
looks over time. It never plans, applies or restores; it only reads `/sys` and
`/proc` and writes to the state directory.

## Each cycle

1. `collect_hardware` → snapshot saved in `snapshots`.
2. `collect_activity` (SSH sessions, `--protect` processes; see
   [GUARDS.md](GUARDS.md)).
3. An `observation` audit event: `snapshot_id`, node, governor,
   `scaling_max_khz`, `load_1m`, `ssh_sessions`, `protected_running`.
4. Snapshot retention: only the newest `--keep-snapshots` (default 1000) per
   node are kept. Audit events that refer to pruned snapshot ids no longer
   resolve; the audit log itself is never pruned.

Because a snapshot is stored every cycle, `--interval 60` with the default
retention keeps about 16 hours of history.

## Lifecycle and audit

- `observer_started` (interval, iterations) → cycles → `observer_stopped`
  (reason, cycles, errors).
- **Stop reasons:** `iterations` (bounded run finished), `stopped` (SIGINT or
  SIGTERM), `too many errors`.
- **Errors:** a failing cycle is audited as `observer_error` and the loop
  carries on. Five failures *in a row* stop it (exit code 1) instead of
  spinning silently. Any successful cycle resets the count.
- **Signals:** the handlers only set a flag, and the sleep between cycles is
  `Event.wait`, so SIGINT/SIGTERM end the loop immediately. A cycle that is
  already running finishes first. Previous handlers are restored on exit.

## CLI

```bash
nordfir observe [--interval 60] [--iterations N] [--keep-snapshots 1000] \
                [--protect PROCESS ...] [--state-dir DIR]
```

`--iterations` makes a bounded run (useful for deployment tests and cron).
Without it the loop runs until signalled. Invalid values are refused (exit 1).
Exit 1 also when it stopped because of repeated errors.

## Running as a service (example, untested)

```ini
[Service]
ExecStart=/opt/nordfir/.venv/bin/nordfir observe --state-dir /var/lib/nordfir
User=nordfir
StateDirectory=nordfir
ProtectSystem=strict
ProtectHome=true
NoNewPrivileges=true
```

The observer needs no root: `/sys` and `/proc` reads are world-readable. The
deployment preflight (step 10) will check this properly.

Code: `src/nordfir/observer.py`, `prune_snapshots` in `db.py`; tests:
`tests/test_observer.py`.
