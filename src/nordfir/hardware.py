"""Read-only collection of local Linux hardware state.

Nothing here opens a file for writing or runs a shell command. The sysfs and
procfs roots are injectable so tests never depend on the host running them.
"""

from __future__ import annotations

import socket
import time
from pathlib import Path

from nordfir.model import CpuFreq, HardwareSnapshot


def collect_hardware(
    sysfs_root: Path | str = "/sys",
    procfs_root: Path | str = "/proc",
    node: str | None = None,
) -> HardwareSnapshot:
    sysfs = Path(sysfs_root)
    procfs = Path(procfs_root)
    cpufreq_dir = sysfs / "devices/system/cpu/cpu0/cpufreq"

    return HardwareSnapshot(
        captured_at=time.time(),
        node=node or socket.gethostname(),
        cpufreq_available=cpufreq_dir.is_dir(),
        cpufreq=_read_cpufreq(cpufreq_dir),
        rapl_available=_rapl_available(sysfs),
        memory_used_percent=_memory_used_percent(procfs / "meminfo"),
        load_1m=_first_float(procfs / "loadavg"),
        uptime_seconds=_first_float(procfs / "uptime"),
    )


def _read_cpufreq(cpufreq_dir: Path) -> CpuFreq:
    governors = _read(cpufreq_dir / "scaling_available_governors")
    return CpuFreq(
        governor=_read(cpufreq_dir / "scaling_governor"),
        available_governors=tuple(governors.split()) if governors else (),
        hardware_min_khz=_read_int(cpufreq_dir / "cpuinfo_min_freq"),
        hardware_max_khz=_read_int(cpufreq_dir / "cpuinfo_max_freq"),
        scaling_min_khz=_read_int(cpufreq_dir / "scaling_min_freq"),
        scaling_max_khz=_read_int(cpufreq_dir / "scaling_max_freq"),
    )


def _rapl_available(sysfs: Path) -> bool:
    try:
        return any("rapl" in entry.name for entry in (sysfs / "class/powercap").iterdir())
    except OSError:
        return False


def _memory_used_percent(meminfo: Path) -> float | None:
    text = _read(meminfo)
    if text is None:
        return None
    values: dict[str, int] = {}
    for line in text.splitlines():
        key, _, rest = line.partition(":")
        parts = rest.split()
        if parts and parts[0].isdigit():
            values[key] = int(parts[0])
    total, available = values.get("MemTotal"), values.get("MemAvailable")
    if not total or available is None:
        return None
    return round((total - available) / total * 100, 1)


def _read(path: Path) -> str | None:
    try:
        value = path.read_text().strip()
    except (OSError, UnicodeDecodeError):
        return None
    return value or None


def _read_int(path: Path) -> int | None:
    value = _read(path)
    try:
        return int(value) if value is not None else None
    except ValueError:
        return None


def _first_float(path: Path) -> float | None:
    value = _read(path)
    try:
        return float(value.split()[0]) if value else None
    except ValueError:
        return None
