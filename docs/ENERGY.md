# Nordfir Energy Model

## Goal

Nordfir optimizes total operational cost, not simply uptime.

The engine should compare the value of entering REST or OFF against startup latency, expected demand and an explicitly uncertain hardware-cycle penalty.

## Initial modes

```text
ACTIVE -> REST -> OFF
   ^        |
   +--------+
      demand
```

REST should normally be preferred for short or uncertain idle periods.

OFF should only be selected when:

- the node is released by intent
- guards allow the transition
- authority permits shutdown
- fresh preflight succeeds
- the expected idle period exceeds the configured minimum
- the estimated energy saving is meaningful

## Break-even

A simple first approximation is:

```text
saving_per_hour = (rest_watts - off_watts) / 1000 * electricity_price
break_even_hours = estimated_cycle_cost / saving_per_hour
```

The cycle cost is a configurable estimate and must not be presented as a precise physical truth.

## Learning without AI

Useful demand prediction can begin with deterministic statistics:

- hour-of-day usage buckets
- weekday/weekend buckets
- exponentially weighted moving averages
- recent wake frequency
- average idle duration
- explicit user availability windows

This keeps the first engine transparent and easy to audit.


## Power intelligence and measurement fallback

Nordfir must not require external metering hardware. External meters are useful
for calibration and high-confidence accounting, but they are optional.

Power data is selected through a provider chain, normally in this order:

1. **External meter** — smart plug, metered PDU, UPS, or another whole-system meter.
2. **System sensor** — BMC, PSU, ACPI, or another platform sensor reporting whole-host power.
3. **Component sensor** — CPU/GPU/package measurements used as partial evidence.
4. **Calibrated estimate** — a local deterministic model fitted against historical wall-power samples.
5. **Generic estimate** — a conservative fallback model when no local calibration exists.

Every reading carries its origin, timestamp, confidence, and optionally an
uncertainty range. Nordfir must never present an estimate as a physical
measurement.

Example:

```text
Power: 147 W ± 9 W
Source: calibrated estimate
Confidence: 0.88
```

### Calibration without AI

The first calibration implementation should use ordinary deterministic
statistics, such as linear regression against wall-power measurements. Inputs
may include CPU utilization, memory utilization, disk activity, and network
activity. This keeps the model explainable, testable, and optional.

A smart plug can therefore be used temporarily to calibrate a node and later
removed. The resulting model remains an estimate and should retain its measured
error bounds.

## v0.4 power source strategy

Nordfir does not require an external smart plug. Power data is represented with
source, scope, confidence, uncertainty and provider metadata.

Recommended provider priority is:

```text
external whole-system meter
  -> whole-system platform/BMC sensor
  -> calibrated estimate
  -> generic estimate
```

Component-only sensors may be useful model inputs but must not silently be
presented as whole-system wall power.

## v0.5 Linux capability discovery

`LinuxPowerProbe` performs read-only discovery under an injectable sysfs root.
It reports:

- whether the cpufreq interface exists;
- the current and available CPU governors;
- hardware and configured frequency ranges;
- whether a RAPL powercap entry exists;
- whether the governor control file exposes write permission bits.

The write-permission observation does not grant authority and does not prove
that the current process may change the file. It is capability evidence only.
Missing or malformed kernel interfaces produce absent values rather than host
changes or guessed defaults.

## v0.5.1 REST change planning

`RestPlanner` converts discovered capabilities and a `PowerProfile` into a
typed `RestChangePlan`. The plan can contain governor and maximum-frequency
changes, but it has no execution behavior.

Planning fails closed when cpufreq, the current governor, the required
`powersave` governor, or frequency-range evidence is missing. The default REST
profile requests a maximum of 40 percent of the hardware maximum, clamped to
the hardware-supported range. It never raises an existing, more restrictive
frequency ceiling.

Plan states are:

- `Ready` — at least one bounded change is proposed;
- `NoChanges` — the REST constraints are already satisfied;
- `Blocked` — required evidence or support is missing.

The CLI prints `Apply: false` and does not pass the plan to a driver.

## v0.5.2 original-state persistence

Before Nordfir gains a writable REST driver, it can capture the current CPU
governor and configured minimum/maximum frequencies in a versioned local state
file. Capture fails if any required value is unknown or invalid.

The file store creates a per-node snapshot and refuses to overwrite it. On
Unix, a newly created state directory uses mode `0700` and a new snapshot uses
mode `0600`. Loading verifies the format, value bounds and expected node. This
is recovery state only: v0.5.2 does not apply or restore it and does not modify
sysfs.
