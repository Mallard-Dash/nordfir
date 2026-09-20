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

### Safe defaults

- Unknown protected-service activity blocks REST.
- Active protected-service activity defers REST.
- Optimization-only authority cannot shut down or reboot nodes.
- No arbitrary shell command execution is present.
- No code in v0.5 writes to sysfs or modifies the host.

### Next milestone

Implement a reversible Linux REST driver behind an explicit opt-in setting.
Original-state capture and persistence are now available, but no saved state is
applied or restored yet. The driver should:
The driver should:

1. require and validate a saved original-state snapshot;
2. apply a bounded REST profile behind explicit opt-in;
3. verify the applied state;
4. restore the original state on `ACTIVE`;
5. fail closed when a required interface is missing;
6. emit audit records for every attempted change.
