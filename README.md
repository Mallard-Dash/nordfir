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

Nordfir modifies host power settings only through explicitly confirmed apply
and restore commands. All other development commands remain read-only or dry-run.

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

The first writable development command is deliberately harder to invoke:

```bash
cargo run -- apply-rest-local ./nordfir-state --confirm-system-power-write
cargo run -- restore-active-local ./nordfir-state --confirm-system-power-write
```

It requires an existing original-state snapshot, a non-blocked REST plan and
write access to Linux cpufreq. It verifies every write and attempts rollback if
a later change fails. `restore-active-local` validates the saved values against
current hardware capabilities before restoring and verifying them. Run these
commands only on a host whose power policy you intend to change.

Writable commands require private state-directory and snapshot permissions.
REST apply additionally requires a snapshot captured within the last 15
minutes. Every confirmed apply or restore attempt appends a result to
`<state-directory>/audit.log`.

After a verified ACTIVE restore, Nordfir retires the active snapshot into the
private `<state-directory>/archive/` directory. The archived recovery point is
preserved, while a new REST cycle can capture a fresh non-overwriting snapshot.

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
