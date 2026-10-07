# Original state (recovery state)

Roadmap step 1. Before Nordfir may change a node, it stores how the node was
configured so a later restore (step 5) can put it back.

## What is captured

From the cpufreq interface of cpu0: governor, scaling min/max and hardware
min/max frequency (kHz), plus node name and capture time.

## Rules

- **Fail closed.** If cpufreq is unavailable, any value is unknown, or the
  limits are inconsistent (`hw_min <= scaling_min <= scaling_max <= hw_max`
  must hold), nothing is stored and the command exits 1.
- **Never overwrite.** One active record per node, enforced by a partial
  unique index in SQLite. A second save is refused until the record is retired
  (retired by a verified [restore](RESTORE.md), which sets `retired_at`).
- **Capture while untouched.** Save it before the first apply. Saving it
  after Nordfir has already lowered the ceiling would record the lowered value
  as "original".

## Storage

Table `original_states` in `nordfir.db` (schema version 2). Opening a v1
database upgrades it in place; the new table is created if missing.

## CLI

```bash
nordfir save-original [--state-dir DIR]      # exit 1 if refused
nordfir show-original [--state-dir DIR] [--node NAME]
```

Both are read-only towards the host; they write only to the state directory.

## Code

`src/nordfir/state.py`: `original_state_from()`, `save_original_state()`,
`load_original_state()`. Tests: `tests/test_state.py`.
