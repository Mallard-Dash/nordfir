# Drivers and dry-run

Roadmap step 3. Drivers are the only components allowed to act on a host. The
engine never touches sysfs directly.

## Contract

```python
class Driver(Protocol):
    name: str
    def apply(self, plan: ChangePlan) -> ApplyResult: ...
```

- A driver receives a finished plan and reports an `ApplyResult`
  (`driver`, `applied`, `actions`, `error`, `rolled_back`). It never decides
  what to do.
- `applied` is `True` if the host may differ from before.
- A driver must refuse a blocked plan (`ValueError`).

## DryRunDriver

Returns the plan's actions with `applied=False` and changes nothing. The real
driver is [LinuxCpufreqDriver](LINUX_DRIVER.md).

## Engine: `execute_plan(conn, driver, plan, operation="apply")`

1. Blocked plan: audits `<operation>_refused` (node, driver, blockers), returns
   `None`, and **never calls the driver**.
2. Otherwise audits `<operation>_started` (planned actions), calls
   `driver.apply(plan)`, then audits `<operation>` (node, driver, applied, error,
   rolled_back, actions). Actions are stored as `{"type": ..., ...fields}`.

## CLI

```bash
nordfir dry-run economize [--state-dir DIR]
```

Plans against the stored original state, runs the dry-run driver, records the
audit event, prints the result and `Apply: false`. Exit 1 if the plan was
refused. It writes only to the state directory.

Code: `src/nordfir/driver.py`, `execute_plan` in `engine.py`; tests:
`tests/test_driver.py`.
