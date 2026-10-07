# Nordfir

Nordfir is a safety-aware energy saver for small servers and home labs.

The project is being restarted in Python. The first Rust implementation
(v0.5.13) is preserved in [`archive/rust/`](archive/rust/README.md), together
with its design documents.

## Design rules carried over from v0.5

- Unknown is not safe: missing information blocks an action.
- Capture the original state before changing anything, and verify every change.
- Prefer leaving a machine online over making an unsafe power-saving decision.
- Never run arbitrary shell commands; only typed, known actions.
- Record what was done and why.
- No AI: every decision is deterministic, inspectable and testable.

## Layout

```text
src/nordfir/
├── model.py      # PowerMode, Intent, HardwareSnapshot, DesiredState
├── hardware.py   # collect_hardware(): read-only /sys and /proc collection
├── audit.py      # append-only, hash-chained audit log
├── db.py         # initiate_db(): private SQLite store for snapshots and audit
├── plan.py       # plan_changes(): typed actions, blockers, no writes
├── guards.py     # SSH / protected-process / staleness guards
├── observer.py   # read-only observe loop with clean shutdown
├── energy.py     # power readings/estimates with source and uncertainty
├── preflight.py  # read-only deployment readiness checks
├── driver.py     # Driver protocol, DryRunDriver, LinuxCpufreqDriver
├── state.py      # original (recovery) state capture, never overwritten
├── engine.py     # desired_state(): decides the target state, fails closed
└── cli.py        # `nordfir` command
```

Only `apply` and `restore` (each needing `--confirm-system-power-write`) change the host (CPU
governor and maximum frequency). Every other command is read-only towards the
host. `nordfir restore --confirm-system-power-write` returns to the saved original
state.

## Development

```bash
python3 -m venv .venv
.venv/bin/pip install -e ".[dev]"
.venv/bin/pytest

.venv/bin/nordfir inspect
.venv/bin/nordfir plan economize            # prints the desired state, Apply: false
.venv/bin/nordfir preflight --role observe   # is this host ready?
.venv/bin/nordfir init-db
.venv/bin/nordfir plan economize --record   # also stores snapshot + decision
.venv/bin/nordfir save-original             # recovery state, refuses to overwrite
.venv/bin/nordfir show-original
.venv/bin/nordfir plan-changes economize    # typed actions, Apply: false
.venv/bin/nordfir dry-run economize         # audits what would be done, changes nothing
# (add --protect NAME, repeatable, to defer while a process runs)
# .venv/bin/nordfir apply economize --confirm-system-power-write   # WRITES to the host
.venv/bin/nordfir observe --iterations 3 --interval 5   # read-only history
.venv/bin/nordfir power                     # best power figure, labelled
.venv/bin/nordfir audit-verify              # check the audit log's hash chain
.venv/bin/nordfir plan-restore              # what restore would do
# .venv/bin/nordfir restore --confirm-system-power-write           # WRITES: back to original
```

## Documentation

- [Roadmap](docs/ROADMAP.md): the step-by-step plan and current progress
- [Architecture](docs/ARCHITECTURE.md): flow, modules, invariants
- [Original state](docs/ORIGINAL_STATE.md): step 1
- [Change plan](docs/CHANGE_PLAN.md): step 2
- [Drivers and dry-run](docs/DRIVERS.md): step 3
- [Linux cpufreq driver](docs/LINUX_DRIVER.md): step 4
- [Restore](docs/RESTORE.md): step 5
- [Safety guards](docs/GUARDS.md): step 6
- [Audit log](docs/AUDIT.md): step 7
- [Observer loop](docs/OBSERVER.md): step 8
- [Energy and power](docs/ENERGY.md): step 9
- [Deployment preflight](docs/PREFLIGHT.md): step 10
