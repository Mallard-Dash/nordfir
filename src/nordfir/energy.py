"""Power readings and estimates, always labelled with where they came from.

Rules carried over from the v0.5 design:

- A reading says its **source**, **scope**, **confidence** and, where known,
  its **uncertainty**. An estimate is never presented as a measurement.
- A *component* reading (CPU package) is never promoted to *system* power.
- Unknown stays unknown: unreadable sensors yield nothing, not zero.

System power is chosen by priority: configured whole-system hwmon sensor,
then a locally calibrated estimate, then a generic estimate.
"""

from __future__ import annotations

import json
import math
import re
import sqlite3
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from nordfir.model import HardwareSnapshot

MIN_CALIBRATION_SAMPLES = 5
_RAPL_DOMAIN = re.compile(r"^[\w-]*rapl:\d+$")  # top-level domains only
_HWMON_SENSOR = re.compile(r"^hwmon\d+/power\d+_(input|average)$")


@dataclass(frozen=True)
class PowerReading:
    watts: float
    source: str  # "hwmon" | "rapl" | "calibrated-estimate" | "generic-estimate"
    scope: str  # "system" | "package"
    confidence: float  # 0..1; a heuristic, not a probability
    uncertainty_watts: float | None
    measured: bool  # False for every estimate
    captured_at: float


@dataclass(frozen=True)
class PowerReport:
    system: PowerReading | None
    components: tuple[PowerReading, ...] = ()
    notes: tuple[str, ...] = ()


# --- sensors ----------------------------------------------------------------

def read_rapl_energy(sysfs_root: Path | str = "/sys") -> dict[str, tuple[int, int | None]]:
    """``{domain: (energy_uj, max_energy_range_uj or None)}`` for readable domains."""
    base = Path(sysfs_root) / "class/powercap"
    out: dict[str, tuple[int, int | None]] = {}
    try:
        entries = sorted(base.iterdir())
    except OSError:
        return out
    for entry in entries:
        if not _RAPL_DOMAIN.match(entry.name):
            continue
        energy = _read_int(entry / "energy_uj")
        if energy is not None:
            out[entry.name] = (energy, _read_int(entry / "max_energy_range_uj"))
    return out


def rapl_power(
    before: dict[str, tuple[int, int | None]],
    after: dict[str, tuple[int, int | None]],
    seconds: float,
    now: float | None = None,
) -> PowerReading | None:
    """Package power from two energy samples. A domain whose counter wrapped
    without a known range is skipped rather than guessed."""
    if seconds <= 0:
        return None
    total_uj = 0
    used = 0
    for domain, (start, _) in before.items():
        if domain not in after:
            continue
        end, max_range = after[domain]
        delta = end - start
        if delta < 0:
            if max_range is None:
                continue
            delta += max_range
        total_uj += delta
        used += 1
    if not used:
        return None
    return PowerReading(
        watts=total_uj / 1e6 / seconds, source="rapl", scope="package",
        confidence=0.9, uncertainty_watts=None, measured=True,
        captured_at=time.time() if now is None else now,
    )


def read_hwmon_power(
    sysfs_root: Path | str, sensor: str, scope: str, now: float | None = None
) -> PowerReading | None:
    """Read a configured hwmon power sensor (microwatts). Only the explicitly
    configured sensor is read; its scope is the operator's claim, not ours."""
    if not _HWMON_SENSOR.match(sensor):
        raise ValueError(f"invalid hwmon sensor {sensor!r} (expected e.g. hwmon2/power1_input)")
    if scope not in ("system", "package"):
        raise ValueError("hwmon scope must be 'system' or 'package'")
    microwatts = _read_int(Path(sysfs_root) / "class/hwmon" / sensor)
    if microwatts is None or microwatts < 0:
        return None
    return PowerReading(
        watts=microwatts / 1e6, source="hwmon", scope=scope,
        confidence=0.8 if scope == "system" else 0.6, uncertainty_watts=None,
        measured=True, captured_at=time.time() if now is None else now,
    )


# --- estimation -------------------------------------------------------------

def cpu_utilization(snapshot: HardwareSnapshot, sysfs_root: Path | str = "/sys") -> float | None:
    """Crude utilization: 1-minute load per CPU, clamped to [0, 1]."""
    if snapshot.load_1m is None:
        return None
    cpus = sum(
        1 for p in (Path(sysfs_root) / "devices/system/cpu").glob("cpu[0-9]*") if p.is_dir()
    )
    if not cpus:
        return None
    return max(0.0, min(snapshot.load_1m / cpus, 1.0))


@dataclass(frozen=True)
class GenericModel:
    """Placeholder numbers for a small x86 server; deliberately wide error bars.
    They are a starting point, not a fact about any machine."""

    idle_watts: float = 35.0
    max_watts: float = 120.0

    def estimate(self, utilization: float, now: float | None = None) -> PowerReading:
        watts = self.idle_watts + (self.max_watts - self.idle_watts) * utilization
        return PowerReading(
            watts=watts, source="generic-estimate", scope="system", confidence=0.3,
            uncertainty_watts=0.4 * self.max_watts, measured=False,
            captured_at=time.time() if now is None else now,
        )


@dataclass(frozen=True)
class CalibrationModel:
    """watts = intercept + slope * utilization, fitted to wall-power samples."""

    intercept_watts: float
    slope_watts: float
    residual_std_watts: float
    r_squared: float
    samples: int

    def estimate(self, utilization: float, now: float | None = None) -> PowerReading:
        return PowerReading(
            watts=max(0.0, self.intercept_watts + self.slope_watts * utilization),
            source="calibrated-estimate", scope="system",
            confidence=round(min(0.95, max(0.1, self.r_squared)), 2),
            uncertainty_watts=2 * self.residual_std_watts, measured=False,
            captured_at=time.time() if now is None else now,
        )


def fit_calibration(samples: list[tuple[float, float]]) -> CalibrationModel:
    """Ordinary least squares of watts on utilization. Refuses weak or odd data."""
    if len(samples) < MIN_CALIBRATION_SAMPLES:
        raise ValueError(f"need at least {MIN_CALIBRATION_SAMPLES} samples, got {len(samples)}")
    for util, watts in samples:
        if not (math.isfinite(util) and math.isfinite(watts)) or not 0 <= util <= 1 or watts <= 0:
            raise ValueError(f"invalid sample: utilization={util}, watts={watts}")
    n = len(samples)
    mean_x = sum(x for x, _ in samples) / n
    mean_y = sum(y for _, y in samples) / n
    sxx = sum((x - mean_x) ** 2 for x, _ in samples)
    if sxx < 1e-6:
        raise ValueError("utilization does not vary across samples; cannot fit")
    slope = sum((x - mean_x) * (y - mean_y) for x, y in samples) / sxx
    if slope < 0:
        raise ValueError("fitted power falls as load rises; samples look wrong")
    intercept = mean_y - slope * mean_x
    residuals = [y - (intercept + slope * x) for x, y in samples]
    sse = sum(r * r for r in residuals)
    syy = sum((y - mean_y) ** 2 for _, y in samples)
    return CalibrationModel(
        intercept_watts=intercept, slope_watts=slope,
        residual_std_watts=math.sqrt(sse / (n - 2)),
        r_squared=1.0 if syy == 0 else 1 - sse / syy, samples=n,
    )


def save_power_model(conn: sqlite3.Connection, node: str, model: CalibrationModel) -> None:
    with conn:
        conn.execute(
            "INSERT INTO power_models (node, created_at, data) VALUES (?, ?, ?)",
            (node, time.time(), json.dumps(asdict(model))),
        )


def load_power_model(conn: sqlite3.Connection, node: str) -> CalibrationModel | None:
    row = conn.execute(
        "SELECT data FROM power_models WHERE node = ? ORDER BY id DESC LIMIT 1", (node,)
    ).fetchone()
    return CalibrationModel(**json.loads(row[0])) if row else None


# --- putting it together ------------------------------------------------------

def power_report(
    snapshot: HardwareSnapshot,
    sysfs_root: Path | str = "/sys",
    *,
    hwmon: tuple[str, str] | None = None,  # (sensor, scope)
    model: CalibrationModel | None = None,
    rapl: PowerReading | None = None,
    generic: GenericModel = GenericModel(),
) -> PowerReport:
    """Pick the best whole-system figure; keep component readings separate."""
    components: list[PowerReading] = []
    notes: list[str] = []
    system: PowerReading | None = None

    if hwmon:
        reading = read_hwmon_power(sysfs_root, *hwmon)
        if reading is None:
            notes.append("configured hwmon sensor is unreadable")
        elif reading.scope == "system":
            system = reading
        else:
            components.append(reading)
    if rapl:
        components.append(rapl)

    if system is None:
        util = cpu_utilization(snapshot, sysfs_root)
        if util is None:
            notes.append("CPU utilization is unknown; no estimate possible")
        elif model:
            system = model.estimate(util)
        else:
            system = generic.estimate(util)
            notes.append("no calibration stored; using the generic estimate")
    if system is not None and not system.measured:
        notes.append("system power is an estimate, not a measurement")
    if components and system is not None and system.scope == "system":
        notes.append("component readings are partial and not part of the system figure")
    return PowerReport(system, tuple(components), tuple(notes))


def _read_int(path: Path) -> int | None:
    try:
        return int(path.read_text().strip())
    except (OSError, ValueError, UnicodeDecodeError):
        return None
