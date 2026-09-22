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
- Secure metadata checks for writable snapshot loads on Unix.
- Fifteen-minute snapshot freshness requirement for REST apply.
- Append-mode local audit records for confirmed apply and restore attempts.
- Non-overwriting archival of recovery state after verified ACTIVE restore.
- Durable directory synchronization around snapshot retirement.
- Read-only recovery lifecycle status for active, archived and audit state.
- Optional fail-closed audit forwarding to a Unix datagram collector.
- Optional externally anchored audit acknowledgement over a Unix stream.
- Explicit-host, read-only REST deployment preflight with aggregated blockers.
- Owner-write validation for existing audit logs before deployment or writes.
- Read-only Linux process-confinement inspection for least-privilege deployment.
- Cohesive module layout without empty one-file placeholders.
- Testable read-only observer service loop with bounded deployment-test mode.

### Safe defaults

- Unknown protected-service activity blocks REST.
- Active protected-service activity defers REST.
- Optimization-only authority cannot shut down or reboot nodes.
- No arbitrary shell command execution is present.
- Read-only and planning commands do not modify the host.
- The only writable command requires the exact `--confirm-system-power-write`
  flag and an existing original-state snapshot.
- Apply refuses stale, future-dated or broadly readable recovery state.
- Restore permits older recovery state so recovery remains possible after a
  long REST interval, but still enforces secure metadata.

### Next milestone

Harden the reversible Linux REST lifecycle for service deployment. Apply,
ACTIVE restoration, audit and recovery-state retirement are now available. The
next work should:

1. exercise apply and restore on hardware that passes deployment preflight;
2. exercise the read-only observer service under the defined least-privilege
   runtime boundary, then add explicit shutdown handling and supervised writes;
3. exercise externally anchored audit receipts with a production collector or
   add cryptographic tamper evidence to the local audit log.
