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
