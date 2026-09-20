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

## Network boundary

Nordfir should be designed to operate entirely on a private network. Internet exposure must never be required for core functionality.
