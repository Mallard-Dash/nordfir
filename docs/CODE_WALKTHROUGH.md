# Nordfir v0.5 Code Walkthrough

This document describes what the current code does. It is intentionally more
implementation-focused than `ARCHITECTURE.md`.

## Scope of v0.5

v0.4 proves the first low-risk vertical path without granting Nordfir the
ability to modify host power settings.

```text
Linux host
  -> read-only state collection
  -> NodeSnapshot
  -> whole-system power estimate
  -> Economize intent
  -> REST safety guard
  -> authority check
  -> typed REST action
  -> dry-run driver
```

Only the explicitly confirmed `apply-rest-local` command writes CPU governor
and maximum-frequency settings. No code invokes shutdown, sends IPMI commands,
or starts/stops services.

## `src/state/linux_power.rs`

`LinuxPowerProbe` discovers cpufreq and RAPL capabilities without opening a
sysfs file for writing. Its sysfs root is injectable, so unit tests use
checked-in fixtures instead of inspecting the machine running the test suite.

The probe treats a missing cpufreq directory and malformed frequency files as
unsupported or unknown state. These conditions do not crash the CLI.

## `src/state/original_power.rs`

`OriginalPowerState` captures the governor and configured minimum/maximum CPU
frequencies needed by a future restore operation. Its on-disk format is
versioned and strictly validated. Invalid tokens, missing or extra fields,
non-finite frequencies, reversed ranges and node mismatches are rejected.

`OriginalPowerStateStore` creates one file per node and uses exclusive file
creation, so capturing state twice cannot silently replace the recovery point.
New Unix directories and files are restricted to the current user. No code in
this module writes to sysfs or restores settings.

## `src/energy/rest_plan.rs`

`RestPlanner` combines discovered Linux capabilities with a `PowerProfile`.
It produces typed governor and frequency-ceiling changes, blockers and
warnings. The result is descriptive only and cannot write to Linux.

The planner requires the configured governor and enough frequency evidence to
calculate a bounded ceiling. It will lower an overly high ceiling but never
raise a ceiling that is already more restrictive.

## `src/state/linux.rs`

`LinuxStateCollector` reads Linux kernel interfaces directly instead of running
shell commands.

Current observations:

- CPU utilization from two `/proc/stat` samples.
- Memory utilization from `/proc/meminfo`.
- One-minute load average from `/proc/loadavg`.
- Uptime from `/proc/uptime`.
- CPU frequency from cpufreq sysfs when available.

Missing information is represented as unavailable/unknown rather than guessed.
The collector currently marks SSH session state as unknown because session
inspection has not been implemented yet.

## `src/core/snapshot.rs`

`NodeSnapshot` is the normalized state passed into the decision engine. It now
contains:

- reachability and current power mode;
- protected services and their activity observations;
- CPU, memory, load, uptime and frequency observations;
- an optional `PowerReading`.

`protected_services` is separate from observed services. This prevents an empty
service map from being interpreted as proof that no protected service exists.

## `src/energy/measurement.rs`

A `PowerReading` does not contain only watts. It also records:

- source kind;
- measurement scope;
- confidence;
- observation time;
- optional uncertainty;
- provider identity.

This allows Nordfir to clearly distinguish a smart plug measurement from a
whole-system sensor, component-only reading, calibrated estimate, or generic
estimate.

## `src/energy/provider.rs`

`PowerProvider` is a read-only interface. `PowerReader` tries configured
providers in explicit priority order and uses the first successful reading.

Recommended order:

1. external whole-system meter;
2. whole-system hardware/BMC sensor;
3. calibrated estimate;
4. generic estimate.

Component-only readings should normally be retained for diagnostics or used as
model inputs rather than presented as total host consumption.

## `src/energy/generic.rs`

`GenericEstimateProvider` is a deterministic fallback. It uses CPU and memory
utilization plus a configured `LinearPowerModel`.

The estimate deliberately has low confidence. Disk and network contributions
are currently zero because v0.4 does not collect those observations yet.

This provider exists so Nordfir remains useful without a smart plug. It is not
intended to pretend that modeled watts are directly measured watts.

## `src/energy/hwmon.rs`

`HwmonPowerProvider` reads a specifically configured Linux `power*_input`
interface. The caller must declare whether that sensor represents the whole
system or only a component. Nordfir does not guess sensor scope from an
arbitrary hwmon filename.

## `src/guards/rest_activity.rs`

`RestActivityGuard` protects workloads when Nordfir considers entering REST.

Rules:

- active protected service -> `DEFER`;
- expected service without an observation -> `BLOCK`;
- unknown/unavailable activity -> `BLOCK`;
- all protected services idle -> `ALLOW`.

REST is therefore not automatically treated as harmless merely because it is
less disruptive than shutdown.

## `src/authority/policy.rs`

`PolicyAuthorityGate` provides the first concrete authority implementation.

`AuthorityPolicy::optimization_only()` grants only:

- `Observe`;
- `OptimizePower`.

It does not grant shutdown, reboot, guard override, service control, or policy
administration. Future master-secret, TOTP and hardware-key verification will
sit behind the same `AuthorityGate` interface rather than changing engine
logic.

## `src/drivers/dry_run.rs`

`DryRunDriver` accepts typed Nordfir actions and records them in memory. It never
changes the host.

This is intentional: the current milestone is to prove observation, safety,
authority, and action selection before implementing a privileged Linux REST
driver.

## `src/drivers/linux_rest.rs`

`LinuxRestDriver` is the first deliberately writable driver. It accepts a
typed, ready REST plan plus the previously saved original state. Its sysfs root
is injectable, allowing tests to exercise writes and rollback in temporary
directories rather than modifying the test host.

Only fixed cpufreq files can be written. The driver checks expected current
values, verifies values after writing and rolls completed changes back in
reverse order if a later operation fails.

The same driver restores `ACTIVE` settings from the validated snapshot. It
checks the saved governor and frequency range against current hardware
capabilities, captures the live pre-restore state and uses that state for
best-effort rollback if restoration fails partway through.

## `src/main.rs`

The binary currently exposes these development commands:

```text
nordfir inspect-local
nordfir power-capabilities-local
nordfir plan-rest-local
nordfir save-original-state-local ./nordfir-state
nordfir show-original-state-local ./nordfir-state
nordfir apply-rest-local ./nordfir-state --confirm-system-power-write
nordfir restore-active-local ./nordfir-state --confirm-system-power-write
nordfir economize-local
```

`inspect-local` reads and prints a local Linux snapshot plus a generic power
estimate.

`power-capabilities-local` reports the local CPU governor, available governors,
frequency ranges, RAPL presence and control-file permission bits. It explicitly
states that no system settings were changed.

`plan-rest-local` creates and prints a `RestChangePlan`. A blocked plan exits
with failure so automation cannot mistake missing safety evidence for success.
All plan output remains dry-run and ends with `Apply: false`.

`save-original-state-local` captures required cpufreq values and creates a
non-overwriting, per-node recovery snapshot. `show-original-state-local`
validates and displays the saved snapshot. Both commands explicitly state that
no system power settings were changed.

`apply-rest-local` is different: it performs real cpufreq writes. It requires
the exact confirmation flag, a valid saved snapshot, a non-blocked plan and OS
permission to write the kernel interfaces.

`restore-active-local` restores and verifies the original frequency range and
governor. It uses the same explicit confirmation requirement.

`economize-local` creates `Intent::Economize`, evaluates guards and authority,
and sends an allowed REST action to `DryRunDriver`. The command explicitly
prints that no system settings were changed.

These commands are development probes, not the final Nordfir CLI contract.

## Deliberately not implemented yet

The following items are intentionally deferred:

- shutdown/reboot;
- IPMI/WOL execution;
- SSH session detection;
- service activity providers such as Jellyfin;
- external meter integrations;
- calibration training from meter history;
- persistent audit storage;
- scheduling and demand prediction;
- master-secret/2FA/hardware-key verification.

The next safe implementation step is durable audit logging plus snapshot
ownership and freshness checks.

## Development-only generic power coefficients

The coefficients currently constructed in `src/main.rs` are demonstration
values only. They are not hardware defaults and must not be used for billing,
capacity planning, or claims about real wall power. A later configuration and
calibration layer will provide node-specific coefficients or replace the model
with a higher-quality provider.
