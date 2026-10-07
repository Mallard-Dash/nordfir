# Linux cpufreq driver

Roadmap step 4. `LinuxCpufreqDriver` is the first component that **writes to
the host**: it sets the cpufreq governor and the maximum scaling frequency.
Undo changes with `nordfir restore` ([RESTORE.md](RESTORE.md)), which uses
this same driver.

## Safety properties

1. **Explicit confirmation.** The driver cannot be constructed without
   `confirmed=True`; the CLI passes it only with
   `--confirm-system-power-write`.
2. **Recovery first.** The plan is blocked unless original state is stored
   ([CHANGE_PLAN.md](CHANGE_PLAN.md)); a blocked plan never reaches the driver.
3. **Drift check.** Before the first write, every action's `expected` value
   is verified on **every** CPU's cpufreq directory. Any mismatch refuses the
   whole plan with nothing written (`error: "drift: ..."`).
4. **No file creation.** Only existing cpufreq files are written.
5. **Read-after-write.** Every write is read back and must equal the target.
6. **Rollback.** On any failure, values already written are restored in
   reverse order. The result reports the outcome:

| Situation                     | `applied` | `rolled_back` | `error`                     |
|-------------------------------|-----------|---------------|-----------------------------|
| success                       | true      | false         | none                        |
| nothing to change             | false     | false         | none                        |
| drift / no policies           | false     | false         | set                         |
| failure, rollback succeeded   | false     | true          | set                         |
| failure, rollback incomplete  | **true**  | false         | set, lists the stuck files  |

`applied=true` with an error means the host may be in a half-changed state.
The audit log names the files.

## Audit

`execute_plan` writes `apply_started` (the planned actions) *before* calling
the driver, then `apply` with the outcome (`applied`, `error`, `rolled_back`).
A crash mid-write therefore leaves a started-but-unfinished trail.

## CLI

```bash
nordfir save-original
nordfir apply economize --confirm-system-power-write   # WRITES
```

Without the flag the command refuses and exits 1. Exit 1 also on a blocked
plan or any driver error.

## Limits (by design, for now)

- Only governor and `scaling_max_freq` are written, for all `cpu*/cpufreq`.
- Observation (`collect_hardware`) still reads cpu0 only; the drift check is
  what guards the other CPUs.
- **Hybrid or mixed CPUs** (e.g. P-cores and E-cores with different frequency
  limits) will usually be *refused* with a `drift` error, because the plan is
  computed from cpu0 and every CPU must match its `expected` value. That is the
  safe failure; per-policy planning is future work.
- No staleness check of the plan yet (step 6).

Code: `LinuxCpufreqDriver` in `src/nordfir/driver.py`; tests:
`tests/test_linux_driver.py` (all against a temp copy of the fixture sysfs).
