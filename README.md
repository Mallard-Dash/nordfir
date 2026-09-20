# Nordfir Engine v0.5

Nordfir is a safety-aware lifecycle and energy control engine for small
infrastructure. It is being built in Rust and intentionally has no integrated
AI dependency.

The v0.5 milestone extends the first read-only/dry-run vertical slice with
Linux power-capability discovery, REST planning and durable original-state
snapshots:

```text
Linux state -> Snapshot -> Power estimate -> Economize intent
            -> Safety -> Authority -> REST action -> Dry run
```

Nordfir does **not** modify host power settings in this version.

## Development commands

```bash
cargo run -- inspect-local
cargo run -- power-capabilities-local
cargo run -- plan-rest-local
cargo run -- save-original-state-local ./nordfir-state
cargo run -- show-original-state-local ./nordfir-state
cargo run -- economize-local
```

`inspect-local` reads local Linux state and prints a generic power estimate.
`power-capabilities-local` inspects cpufreq and RAPL interfaces without writing
to them. `plan-rest-local` creates a typed, non-executable REST change plan.
`save-original-state-local` records the current governor and configured CPU
frequency range without overwriting an existing snapshot. `show-original-state-local`
validates and displays that snapshot.
`economize-local` evaluates an `Economize` intent and records the resulting
REST action through a non-destructive dry-run driver.

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
