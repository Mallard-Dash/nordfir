# Implementation Status

## v0.4 milestone

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

### Safe defaults

- Unknown protected-service activity blocks REST.
- Active protected-service activity defers REST.
- Optimization-only authority cannot shut down or reboot nodes.
- No arbitrary shell command execution is present.
- No code in v0.4 modifies the host.

### Next milestone

Implement a reversible Linux REST driver behind an explicit opt-in setting.
The driver should:

1. discover supported CPU power controls;
2. record the original settings;
3. apply a bounded REST profile;
4. verify the applied state;
5. restore the original state on `ACTIVE`;
6. fail closed when a required interface is missing;
7. emit audit records for every attempted change.
