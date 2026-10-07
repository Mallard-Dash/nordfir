# Change plan

Roadmap step 2. Turns a `DesiredState` into the concrete, ordered actions
needed to reach it. Planning is pure and writes nothing.

## Actions

| Action           | Fields                          | Meaning                       |
|------------------|---------------------------------|-------------------------------|
| `SetGovernor`    | `expected`, `target`            | change the cpufreq governor   |
| `SetScalingMax`  | `expected_khz`, `target_khz`    | change the frequency ceiling  |

`expected` is the value seen when planning. A driver must re-read the host and
refuse to act if it no longer matches (drift check, step 4).

Actions that would change nothing are omitted. A plan with no actions and no
blockers means "already there".

## Ordering

Governor first, then ceiling.

## Blockers

A plan with blockers carries **no actions**. Blockers come from:

- the desired state itself (unknown values, unsupported intent), passed through;
- no stored original state (recovery before change; see
  [ORIGINAL_STATE.md](ORIGINAL_STATE.md));
- original state for a different node, or a hardware frequency range that
  differs from the stored one (the hardware changed since capture);
- unknown current minimum frequency, or a target ceiling below it (Linux would
  reject that write).

A plan can also carry `deferrals` (the node is busy; see
[GUARDS.md](GUARDS.md)). Both make `plan.blocked` true.

Blockers are only raised when there is something to change; a node already at
target needs no recovery state.

## Intents

Only REST produces actions. `available` and `maintenance` plan nothing;
returning to ACTIVE from the saved original state is step 5.

## CLI

```bash
nordfir plan-changes economize [--state-dir DIR]   # prints plan, "Apply: false"
```

Exit code 1 if blocked. Code: `src/nordfir/plan.py`; tests:
`tests/test_plan.py`.
