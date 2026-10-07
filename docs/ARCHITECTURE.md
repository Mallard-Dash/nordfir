# Architecture

## Flow

```text
observe            decide          plan          guard          act
collect_hardware → desired_state → plan_changes → guard_plan → driver.apply → verify
       │                │                  │
       └──────── SQLite: snapshots, audit_events, original_states
```

Observe, decide and plan write nothing to the host. The act stage goes through
a `Driver`: `DryRunDriver` changes nothing, `LinuxCpufreqDriver` writes cpufreq;
restore goes through the same driver.

## Modules

| Module        | Responsibility                                              |
|---------------|-------------------------------------------------------------|
| `model.py`    | Frozen dataclasses and enums. Unknown values are `None`.    |
| `hardware.py` | Read-only `/sys` and `/proc` collection, roots injectable.  |
| `engine.py`   | Pure `desired_state()`; `run_once`, `execute_plan`, `restore_original`. |
| `db.py`       | Private SQLite store (dir 0700, file 0600), schema version. |
| `audit.py`    | Append-only, hash-chained audit log and `verify_audit`.     |
| `state.py`    | Original-state (recovery) capture and storage.              |
| `plan.py`     | Pure `plan_changes()`, `plan_restore()`: typed actions.     |
| `guards.py`   | SSH / protected-process / staleness guards (apply only).    |
| `observer.py` | Read-only loop: snapshots + observation events, stoppable.  |
| `energy.py`   | Labelled power readings, calibration, system-power choice.  |
| `preflight.py`| Read-only host readiness report (`observe` / `apply`).      |
| `driver.py`   | `Driver`, `DryRunDriver`, `LinuxCpufreqDriver`: only way to act. |
| `cli.py`      | `nordfir` command; `apply`/`restore` can write.            |

## Invariants

- **Unknown is not safe.** A `None` where a value is needed produces a
  blocker, never a guess.
- **Pure decisions.** `desired_state()` takes a snapshot and returns a value;
  it does no I/O, so every decision is reproducible from a stored snapshot.
- **Recovery before change.** No write happens until original state is
  stored: `plan_changes` blocks otherwise, and drivers refuse blocked plans.
- **Not while busy.** `guard_plan` turns active SSH sessions or protected
  processes into deferrals and unknown activity into blockers. Restore skips it.
- **Confirmed writes only.** `LinuxCpufreqDriver` cannot be built without
  `confirmed=True`, which the CLI sets only for `--confirm-system-power-write`.
- **Tamper-evident audit.** Events are append-only (triggers) and hash-chained;
  `nordfir audit-verify` checks the chain. See [AUDIT.md](AUDIT.md).
- **Looking leaves no trace.** Commands that only read stored state
  (`plan-changes`, `plan-restore`, `show-original`, `audit-verify`, `power`)
  never create the state directory; `preflight` never writes at all.
- **Schema versioning.** `PRAGMA user_version` tracks the schema; opening a
  database from a newer version fails rather than guessing.

## Testing

Tests run against fixture trees in `tests/fixtures/` passed as `sysfs_root` /
`procfs_root`, so they never depend on the host's hardware.
