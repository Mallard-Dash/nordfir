# Roadmap

Nordfir is being rebuilt in Python, using the archived Rust v0.5 design
(`archive/rust/docs/`) as the reference for *what* to build and the Python
code as the source of truth for *how*.

Each step is small, ends with passing tests and updated docs, and leaves the
project safe to stop at. Tick a step only when its code, tests and docs are in.

## Done

- [x] **0. Read-only core**: `collect_hardware`, `desired_state`, SQLite
  snapshot + audit store, `inspect` / `plan` / `init-db` CLI.

## Next

- [x] **1. Original-state snapshot**: capture the governor and frequency
  limits *before* any change and store them as recovery state. Refuse to
  overwrite existing recovery state. CLI: `save-original`, `show-original`.
  See [ORIGINAL_STATE.md](ORIGINAL_STATE.md).
- [x] **2. Typed change plan**: turn a `DesiredState` plus the original state
  into an ordered list of typed actions (`SetGovernor`, `SetScalingMax`)
  with expected before-values. Still no writes. See
  [CHANGE_PLAN.md](CHANGE_PLAN.md).
- [x] **3. Dry-run driver**: a `Driver` protocol and a `DryRunDriver` that
  records what it *would* do to the audit log. The engine calls drivers, never
  sysfs directly. See [DRIVERS.md](DRIVERS.md).
- [x] **4. Linux REST driver**: explicit opt-in writes to cpufreq with
  expected-state checks, read-after-write verification and best-effort
  rollback. Requires `--confirm-system-power-write` and a saved original
  state. See [LINUX_DRIVER.md](LINUX_DRIVER.md).
- [x] **5. Restore**: return to ACTIVE using the saved original state, with
  hardware-bound validation and verification, then retire the recovery state.
  See [RESTORE.md](RESTORE.md).
- [x] **6. Safety guards**: block or defer REST when protected services are
  active, SSH sessions exist, or the snapshot is stale. Unknown blocks.
  See [GUARDS.md](GUARDS.md).
- [x] **7. Audit hardening**: append-only triggers, hash-chained events for
  tamper evidence, distinct restore events, `audit-verify`.
  See [AUDIT.md](AUDIT.md).
- [ ] **7b. Audit forwarding**: optional external sink (e.g. Unix datagram),
  fail-closed before writes, to cover events removed from the end of the log.
- [x] **8. Observer loop**: a bounded, read-only service loop that observes and
  records on an interval, with clean shutdown handling.
  See [OBSERVER.md](OBSERVER.md).
- [x] **9. Energy estimation**: RAPL and hwmon power readings with source,
  confidence and uncertainty; deterministic whole-system estimate.
  See [ENERGY.md](ENERGY.md).
- [x] **10. Deployment preflight**: read-only check that a host is ready
  (permissions, capabilities, confinement), aggregating all blockers.
  See [PREFLIGHT.md](PREFLIGHT.md).

## After step 10

Open work, roughly in order of value:

- **7b. Audit forwarding** (above).
- **Try it on hardware.** Run `preflight`, `save-original`, `apply`, `restore`
  on a real machine with write access and check every verification path.
- **Use the power figure:** REST saving estimates and break-even for OFF.
- **OFF mode** (`release` intent): currently always blocked.
- **Config file** for protected processes, thresholds and the SSH port.
- Record power readings in observer cycles.

## Rules for every step

1. Fail closed: unknown input blocks an action, it never defaults.
2. No arbitrary shell commands; only typed actions.
3. Hardware access goes through an injectable root so tests use fixtures.
4. Docs are part of the change: update this roadmap, `ARCHITECTURE.md`, and
   add or update the step's own doc in the same commit.
