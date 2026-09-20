# Nordfir Engine Scaffold v0.4

Nordfir is a safety-aware lifecycle and energy control engine for small
infrastructure. It is being built in Rust and intentionally has no integrated
AI dependency.

The v0.4 milestone implements the first read-only/dry-run vertical slice:

```text
Linux state -> Snapshot -> Power estimate -> Economize intent
            -> Safety -> Authority -> REST action -> Dry run
```

Nordfir does **not** modify host power settings in this version.

## Development commands

```bash
cargo run -- inspect-local
cargo run -- economize-local
```

The first command reads local Linux state and prints a generic power estimate.
The second evaluates an `Economize` intent and records the resulting REST action
through a non-destructive dry-run driver.

## Documentation

- `docs/ARCHITECTURE.md` — subsystem boundaries and the one-engine model.
- `docs/ENGINE.md` — core engine concepts.
- `docs/SECURITY.md` — authority and safety principles.
- `docs/ENERGY.md` — energy model and measurement strategy.
- `docs/CODE_WALKTHROUGH.md` — what the current code actually does.
- `docs/IMPLEMENTATION_STATUS.md` — implemented/deferred functionality.

## Design rule

Nordfir must never receive more authority than required for the selected
operating mode. Unknown safety-critical state is not treated as safe.
