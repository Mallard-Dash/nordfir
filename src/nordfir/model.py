"""Shared domain types.

Unknown values are always ``None``; they are never replaced by guessed defaults.
Frequencies are kept in kHz, the unit Linux cpufreq uses.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class PowerMode(str, Enum):
    ACTIVE = "active"
    REST = "rest"
    OFF = "off"


class Intent(str, Enum):
    AVAILABLE = "available"
    ECONOMIZE = "economize"
    RELEASE = "release"
    MAINTENANCE = "maintenance"


@dataclass(frozen=True)
class CpuFreq:
    governor: str | None = None
    available_governors: tuple[str, ...] = ()
    hardware_min_khz: int | None = None
    hardware_max_khz: int | None = None
    scaling_min_khz: int | None = None
    scaling_max_khz: int | None = None


@dataclass(frozen=True)
class HardwareSnapshot:
    captured_at: float
    node: str
    cpufreq_available: bool
    cpufreq: CpuFreq
    rapl_available: bool
    memory_used_percent: float | None = None
    load_1m: float | None = None
    uptime_seconds: float | None = None


@dataclass(frozen=True)
class DesiredState:
    """The target power state for a node, or the reasons it cannot be decided."""

    intent: Intent
    mode: PowerMode | None
    governor: str | None = None
    scaling_max_khz: int | None = None
    blockers: tuple[str, ...] = field(default_factory=tuple)

    @property
    def blocked(self) -> bool:
        return bool(self.blockers)
