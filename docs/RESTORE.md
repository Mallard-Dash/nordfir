# Restore

Roadmap step 5. Returns a node to the configuration saved by
[`save-original`](ORIGINAL_STATE.md), then retires that record.

## Planning: `plan_restore(snapshot, original)`

Produces the same typed actions as [CHANGE_PLAN.md](CHANGE_PLAN.md)
(`SetGovernor`, then `SetScalingMax`) from the *current* values to the
original ones. It is validated against the hardware as it is **now** and is
blocked (no actions) when:

- no original state is stored;
- the state belongs to a different node;
- cpufreq is unavailable, or the current governor / ceiling is unknown;
- the hardware frequency range differs from the stored one (the machine
  changed since capture, so the old values may be wrong);
- the original ceiling is below the current minimum frequency.

If the host already matches, the plan is empty and not blocked.

Unlike `apply`, restore does **not** require a fresh plan or a recent
snapshot: recovery must stay possible after a long REST period.

## Execution: `restore_original(conn, driver, snapshot)`

Runs the plan through `execute_plan`, so it gets the driver's drift check,
read-after-write verification and rollback, and the same `restore_started` /
`restore` / `restore_refused` audit events (see [AUDIT.md](AUDIT.md)), followed
by `original_state_retired` on success.

The record is **retired** (`retired_at` set, row kept, `original_state_retired`
audited) only when the host is verifiably back at the original values:

| Outcome                                   | Retired? |
|-------------------------------------------|----------|
| driver wrote and verified, no error       | yes      |
| already at original, driver can write     | yes      |
| dry-run driver (`writes_host = False`)    | no       |
| driver error, with or without rollback    | no       |
| blocked plan                              | no       |

After retirement, `save-original` can capture a fresh state.

## CLI

```bash
nordfir plan-restore [--state-dir DIR]                         # read-only, Apply: false
nordfir restore --confirm-system-power-write [--state-dir DIR] # WRITES
```

`restore` refuses without the flag. Exit 1 on refusal, blocked plan or driver
error.

## Typical cycle

```bash
nordfir save-original
nordfir apply economize --confirm-system-power-write
nordfir restore --confirm-system-power-write
```

Code: `plan_restore` in `plan.py`, `retire_original_state` in `state.py`,
`restore_original` in `engine.py`; tests: `tests/test_restore.py`.
