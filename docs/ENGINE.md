# Nordfir Engine Architecture

## Purpose

Nordfir is a safety-aware lifecycle and energy control engine for small infrastructure.

Its primary purpose is to reduce unnecessary energy use without treating power-off as the only optimization. Nordfir should prefer a low-power `REST` state when that satisfies availability requirements, and only consider `OFF` when the expected savings justify the disruption and the user has explicitly granted that authority.

Nordfir is not a Kubernetes replacement. It may borrow proven distributed-systems ideas, but it deliberately uses its own vocabulary and focuses on physical node lifecycle, application activity, energy cost, authority boundaries, and explainable control.

Nordfir is designed to work without an integrated AI system. Core decisions must be deterministic, inspectable, reproducible and testable.

## Core flow

```text
Watchers
   |
   v
Snapshot
   |
   v
Intent
   |
   v
Candidate Action
   |
   v
Guards
   |
   v
Authority Gate
   |
   v
Preflight
   |
   v
Driver
   |
   v
Verification + Audit
```

### Intent

Intent describes a requirement rather than a command implementation.

Initial node-level intents are:

- `Available` — keep the node immediately usable.
- `Economize` — reduce energy use while preserving required reachability and constraints.
- `Release` — the node is no longer required to remain available; power-off may be considered.
- `Maintenance` — suppress automatic optimization while maintenance is in progress.

### Power modes

Nordfir starts with three power modes:

- `ACTIVE` — unrestricted normal operation.
- `REST` — low-power residency while the OS and control path remain available.
- `OFF` — node is intentionally powered down.

`REST` is expected to be the default optimization target. `OFF` is an aggressive optimization and requires stronger evidence and authority.

Nordfir must not attempt to replace the Linux CPU idle governor. A REST profile should set high-level policy such as a CPU ceiling and service behavior, while the operating system remains responsible for low-level C-state management.

### Guards and preflight

Guards answer whether an action is allowed by policy. Preflight answers whether a previously allowed disruptive action is still safe immediately before execution.

This separation protects against stale observations and time-of-check/time-of-use failures.

A destructive action must never rely only on a dashboard cache.

### Authority

Authority is separate from safety policy.

A safe action is not automatically an authorized action.

Capabilities are granular, for example:

- observe state
- optimize power
- control approved services
- suspend a node
- shut down a node
- reboot a node
- edit timing
- edit protection rules
- override guards
- manage authority

An installation may therefore run Nordfir in observation-only or optimization-only mode without granting shutdown privileges.

Step-up authentication can later be required for sensitive capability changes or guard overrides. The scaffold includes abstractions for master secrets, recovery phrases, TOTP and hardware keys, but intentionally implements no custom cryptography.

Future authentication implementations should use established protocols and libraries. Secrets should not be stored in plaintext configuration.

### Energy model

Nordfir should reason about energy optimization rather than blindly minimizing uptime.

Useful inputs include:

- measured ACTIVE idle power
- measured REST power
- measured OFF standby power
- electricity price
- expected idle duration
- minimum useful off-time
- estimated cycle penalty
- probability of near-future demand

The initial scaffold includes only a simple break-even estimator. Any hardware-wear estimate must be presented as an estimate, not as a known physical cost.

### Deterministic first

Nordfir does not require AI to learn useful behavior.

A first implementation can use:

- rolling averages
- time-of-day histograms
- recent activity windows
- explicit availability windows
- measured break-even times
- deterministic rules

If predictive models are added in the future, they should remain optional and must never bypass safety or authority checks.

## Safety rules

1. Unknown is not safe.
2. Stale observations must not authorize disruptive actions.
3. Expected protected services must be explicitly known before shutdown.
4. Active sessions and active protected services block automatic shutdown by default.
5. Destructive actions require a fresh preflight.
6. A driver accepts typed actions, never arbitrary user-provided shell commands.
7. Authority is capability-based and should follow least privilege.
8. Guard overrides must be explicit, authenticated and audited.
9. Critical guards may be non-overridable.
10. Nordfir must prefer leaving hardware online over making an unsafe power-saving decision.

## Module layout

```text
src/
├── action.rs
├── engine.rs
├── core/
│   ├── intent.rs
│   ├── node.rs
│   ├── observation.rs
│   ├── power_mode.rs
│   ├── service.rs
│   └── snapshot.rs
├── authority/
│   ├── capability.rs
│   ├── challenge.rs
│   └── policy.rs
├── energy/
│   ├── estimate.rs
│   ├── model.rs
│   └── profile.rs
├── guards/
│   ├── active_service.rs
│   ├── freshness.rs
│   └── ssh_session.rs
├── preflight/
├── watchers/
├── drivers/
├── timing/
└── audit/
```

## Design boundary

Nordfir should coordinate existing systems rather than replace them.

Linux manages CPU idle states. systemd, Docker, Proxmox or Kubernetes may own workloads. IPMI or Redfish may own out-of-band power. Nordfir observes those systems, applies explicit policy, and decides when a lifecycle transition is justified and authorized.
