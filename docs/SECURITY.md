# Nordfir Security Model

## Trust principle

Nordfir must never receive more authority than required for the selected operating mode.

A user should be able to install Nordfir with read-only authority and later grant individual capabilities deliberately.

## Authority levels

Suggested presets:

### Observe

- read node state
- read energy metrics
- read application activity

### Optimize

- everything in Observe
- apply approved REST power profiles
- control explicitly approved non-critical services

### Power

- everything in Optimize
- wake, suspend, shut down and reboot nodes according to policy

Presets are UI conveniences only. Internally, capabilities remain granular.

## Step-up authentication

Sensitive changes should support step-up authentication.

Possible factors include:

- master secret
- recovery phrase
- TOTP
- hardware security key such as a FIDO2/WebAuthn-compatible device

The engine must not invent its own password hashing, challenge-response or hardware-key protocol. Authentication providers should be isolated behind `AuthorityGate`.

Recommended future rule:

```text
changing power authority      -> step-up required
editing critical guards       -> step-up required
overriding hard guard         -> step-up required
managing authentication       -> strongest available step-up
```

## No arbitrary shell execution

Drivers receive typed actions. API clients and users must not be able to submit arbitrary shell strings through the engine.

Service identifiers are resolved against trusted configuration before execution.

## Audit

The following should eventually be append-only audit events:

- authority changes
- failed authentication
- guard overrides
- disruptive actions
- policy changes
- preflight failures

## Recovery state

Original power settings must be captured before a writable REST transition.
Nordfir must refuse to overwrite an existing recovery snapshot automatically
and must verify its version, values and node identity before use. Newly created
local state should be readable only by the service account. Writable loads
reject symlinks, broad group/other permissions and ownership mismatches between
the state file and directory. Apply requires recent state; restore permits old
state so emergency recovery does not expire.

## Writable REST transition

The Linux REST driver accepts only typed governor and maximum-frequency
changes and resolves them to fixed cpufreq paths. The CLI requires an exact
confirmation flag, but that flag is not an authentication factor. Operating
system permissions remain the enforcement boundary.

Every write must be preceded by an expected-state check and followed by a
read-back verification. Partial failure triggers best-effort rollback, and a
rollback failure must be surfaced to the caller. Production use additionally
requires tamper-evident or externally forwarded audit records.

ACTIVE restoration validates saved values against current hardware
capabilities and captures the live state before writing. A partial restoration
attempts to roll back to that captured live state rather than assuming the host
was still in the default REST profile.

## Local audit log

Confirmed apply and restore operations append intent and outcome events to a
private local audit file. Control characters in fields are escaped, and unsafe
audit paths or permissions fail closed before a power write. OS append mode
prevents accidental truncation but is not cryptographic tamper evidence. A
future deployment should forward records to a separate trust boundary or add a
verifiable hash chain.

`NORDFIR_AUDIT_FORWARD_SOCKET` enables a narrow handoff to a separate collector
through an existing Unix datagram socket. Nordfir validates that the absolute
destination is a socket, attempts both the local and forwarding sinks, and
surfaces every delivery failure. If the initial intent cannot reach both sinks,
the requested power write does not begin. The collector, not Nordfir, owns any
network credentials and remote durable storage.

## Recovery-state retirement

An active recovery snapshot is archived only after ACTIVE settings have been
restored and verified. Retirement never overwrites an existing archive name.
The archive directory remains private, and directory metadata is synchronized
around the transition. Failure leaves recovery data available and is surfaced
through both the command result and audit path.

The read-only lifecycle status command applies the same metadata validation to
active recovery state, node-specific archives and the audit path. It does not
repair, create or otherwise mutate those artifacts.

## Network boundary

Nordfir should be designed to operate entirely on a private network. Internet exposure must never be required for core functionality.
