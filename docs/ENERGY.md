# Energy and power estimation

Roadmap step 9. Gives Nordfir a power figure it can show and later reason
about, **always labelled with where it came from**. Nothing here changes the
host, and nothing yet uses the figure to decide (savings and break-even
arithmetic come after this step).

See `archive/rust/docs/ENERGY.md` for the original design this follows.

## A reading says what it is

`PowerReading`: `watts`, `source`, `scope`, `confidence`, `uncertainty_watts`,
`measured`, `captured_at`.

| `source`              | `scope`           | `measured` | Comes from                         |
|-----------------------|-------------------|------------|------------------------------------|
| `hwmon`               | as configured     | yes        | a sensor you name explicitly       |
| `rapl`                | `package`         | yes        | CPU energy counter, two samples    |
| `calibrated-estimate` | `system`          | **no**     | regression on your wall samples    |
| `generic-estimate`    | `system`          | **no**     | built-in placeholder model         |

`confidence` is a heuristic between 0 and 1, **not a probability**.

## Rules

- **An estimate is never a measurement** (`measured=false`, and the report adds
  a note).
- **Component is never system.** RAPL is CPU package power only. A hwmon
  sensor is `package` unless you declare `--hwmon-scope system`; a package
  reading goes in `components` and never becomes the system figure.
- **Unknown stays unknown.** Unreadable sensors give nothing, not 0 W. If
  utilization is unknown and no sensor reads, `system` is `null`.
- **No guessing which sensor.** hwmon is read only when named, and the name
  must match `hwmonN/powerM_input|average`, so a path cannot escape
  `/sys/class/hwmon`.

## Choosing the system figure

1. a hwmon sensor configured with scope `system`;
2. otherwise the **calibrated estimate**, if a model is stored for the node;
3. otherwise the **generic estimate** (35 W idle to 120 W at full load, ±40% of
   the maximum, confidence 0.3). These are placeholders, not facts about your
   machine.

Both estimates use **utilization = 1-minute load average / CPU count, clamped
to 0..1**. That is crude; it is what the snapshot offers today.

## RAPL

`read_rapl_energy` reads `energy_uj` of top-level `intel-rapl:N` domains;
`rapl_power` turns two samples into watts. A wrapped counter is corrected with
`max_energy_range_uj`; without it the domain is skipped. On many kernels
`energy_uj` is readable by root only, so RAPL is often simply absent for an
unprivileged user; that is reported as no component reading.

## Calibration (no AI)

Ordinary least squares of watts on utilization, from samples you take with a
wall meter. `nordfir calibrate samples.csv` expects columns
`utilization,watts` (utilization 0 to 1). The model stores intercept, slope,
residual standard deviation, R² and sample count; the estimate's uncertainty
is **twice the residual standard deviation**.

It is refused (exit 1, nothing stored) when:

- fewer than 5 samples;
- any value is not finite, utilization is outside 0..1, or watts ≤ 0;
- utilization does not vary (nothing to fit);
- the slope is negative (power falling as load rises means bad samples).

The newest model per node is used. Stored in table `power_models` (schema v4;
older databases upgrade on open).

## CLI

```bash
nordfir calibrate samples.csv
nordfir power                                  # best figure + notes
nordfir power --hwmon hwmon2/power1_input --hwmon-scope system
nordfir power --sample-seconds 0               # skip the RAPL window
```

`power` samples RAPL over `--sample-seconds` (default 1) and exits 1 if there
is no system figure.

Code: `src/nordfir/energy.py`; tests: `tests/test_energy.py`.

## Not yet

- Using the figure: REST saving estimates, break-even for OFF.
- Disk and network inputs to the model; per-frequency effects.
- Recording power in observer cycles.
