# Implementation Status

## v0.5 milestone

The current milestone is intentionally conservative.

### Implemented

- Read-only local Linux state collection.
- Typed observations with timestamps and sources.
- Node snapshot fields for CPU, memory, load, uptime, CPU frequency, services and power.
- Explicit protected-service inventory in snapshots.
- Power readings with source, scope, confidence and uncertainty.
- Configurable Linux hwmon power provider.
- Deterministic generic whole-system power estimation.
- REST activity safety guard.
- Concrete policy-based authority gate.
- Optimization-only authority preset.
- Non-destructive dry-run driver.
- Local inspect/economize development commands.
- Read-only Linux cpufreq and RAPL capability discovery.
- Injectable sysfs root with fixture-based capability tests.
- Local `power-capabilities-local` development command.
- Deterministic, typed REST change planning.
- Fail-closed planning when required cpufreq evidence is missing.
- Local `plan-rest-local` development command with `Apply: false`.
- Versioned original-power-state snapshot format with strict validation.
- File-backed local snapshot storage that refuses to overwrite existing state.
- Local commands to save and validate/display an original-state snapshot.
- Explicit opt-in Linux REST driver for governor and maximum-frequency writes.
- Expected-state checks, read-after-write verification and best-effort rollback.
- Explicit ACTIVE restoration of the saved frequency range and governor.
- Hardware-bound validation and read-after-write verification during restore.

### Safe defaults

- Unknown protected-service activity blocks REST.
- Active protected-service activity defers REST.
- Optimization-only authority cannot shut down or reboot nodes.
- No arbitrary shell command execution is present.
- Read-only and planning commands do not modify the host.
- The only writable command requires the exact `--confirm-system-power-write`
  flag and an existing original-state snapshot.

### Next milestone

Harden the reversible Linux REST lifecycle. Apply and ACTIVE restoration are
available behind explicit opt-in, but durable audit is not yet exposed. The
next work should:

1. verify snapshot ownership and freshness before writable operations;
2. emit durable audit records for every attempted change;
3. exercise apply and restore on explicitly selected test hardware.
