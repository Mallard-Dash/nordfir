"""Deployment preflight: is this host ready to run Nordfir?

Strictly read-only: it never creates a directory, opens the database for
writing, or writes a sysfs file (write access is asked of the kernel with
``os.access``, not tried). All problems are collected so one run lists every
blocker.

Status meanings: ``fail`` blocks the chosen role; ``warn`` is worth knowing
but does not block; ``pass`` is fine.
"""

from __future__ import annotations

import os
import sqlite3
import stat
from dataclasses import dataclass
from pathlib import Path

from nordfir.audit import verify_audit
from nordfir.db import SCHEMA_VERSION
from nordfir.hardware import collect_hardware
from nordfir.state import load_original_state, original_state_from

ROLES = ("observe", "apply")


@dataclass(frozen=True)
class Check:
    name: str
    status: str  # "pass" | "warn" | "fail"
    detail: str


@dataclass(frozen=True)
class PreflightReport:
    role: str
    ready: bool
    checks: tuple[Check, ...]


def run_preflight(
    role: str,
    *,
    sysfs_root: Path | str = "/sys",
    procfs_root: Path | str = "/proc",
    state_dir: Path | str,
    uid: int | None = None,
) -> PreflightReport:
    """Check readiness for ``role``: ``observe`` (read-only loop) or ``apply``
    (may also change cpufreq and restore)."""
    if role not in ROLES:
        raise ValueError(f"role must be one of {ROLES}")
    uid = os.geteuid() if uid is None else uid
    sysfs, procfs, state = Path(sysfs_root), Path(procfs_root), Path(state_dir)
    checks: list[Check] = []

    checks += _proc_checks(procfs)
    checks += _state_dir_checks(state, uid)
    checks += _database_checks(state, role)
    checks += _confinement_checks(procfs, role)
    if role == "apply":
        checks += _cpufreq_checks(sysfs, procfs)
        checks += _pending_restore_check(sysfs, procfs, state)
    return PreflightReport(role, all(c.status != "fail" for c in checks), tuple(checks))


# --- checks -------------------------------------------------------------------

def _proc_checks(procfs: Path) -> list[Check]:
    checks = []
    tables = [n for n in ("tcp", "tcp6") if _readable(procfs / "net" / n)]
    checks.append(
        Check("ssh-visibility", "pass", f"can read /proc/net/{', '.join(tables)}")
        if tables
        else Check("ssh-visibility", "fail", "cannot read /proc/net/tcp or tcp6; "
                   "SSH sessions would be unknown and guards would block every change")
    )
    missing = [n for n in ("loadavg", "meminfo", "uptime") if not _readable(procfs / n)]
    checks.append(
        Check("proc-basics", "fail", f"cannot read: {', '.join(missing)}")
        if missing
        else Check("proc-basics", "pass", "loadavg, meminfo, uptime readable")
    )
    return checks


def _state_dir_checks(state: Path, uid: int) -> list[Check]:
    try:
        info = state.stat()
    except FileNotFoundError:
        ancestor = next((p for p in state.parents if p.exists()), None)
        if ancestor is not None and os.access(ancestor, os.W_OK | os.X_OK, effective_ids=True):
            return [Check("state-dir", "pass", f"{state} does not exist yet; will be created (0700)")]
        return [Check("state-dir", "fail", f"{state} does not exist and cannot be created")]
    except OSError as error:
        return [Check("state-dir", "fail", f"cannot inspect {state}: {error}")]

    problems = _mode_problems(state, info, uid, want_dir=True)
    db = state / "nordfir.db"
    if db.exists():
        problems += _mode_problems(db, db.stat(), uid, want_dir=False)
    if not os.access(state, os.W_OK | os.X_OK, effective_ids=True):
        problems.append(f"{state} is not writable")
    if problems:
        return [Check("state-dir", "fail", "; ".join(problems))]
    return [Check("state-dir", "pass", f"{state} is private and owned by uid {uid}")]


def _mode_problems(path: Path, info: os.stat_result, uid: int, *, want_dir: bool) -> list[str]:
    problems = []
    if want_dir and not stat.S_ISDIR(info.st_mode):
        problems.append(f"{path} is not a directory")
    if info.st_uid != uid:
        problems.append(f"{path} is owned by uid {info.st_uid}, not {uid}")
    if info.st_mode & 0o077:
        problems.append(f"{path} is accessible to group/others (mode {stat.S_IMODE(info.st_mode):04o})")
    return problems


def _database_checks(state: Path, role: str) -> list[Check]:
    path = state / "nordfir.db"
    if not path.exists():
        detail = "no database yet; it is created on first use"
        if role == "apply":
            return [Check("database", "pass", detail),
                    Check("original-state", "warn", "no database: run `nordfir save-original` before apply")]
        return [Check("database", "pass", detail)]
    try:
        conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    except sqlite3.Error as error:
        return [Check("database", "fail", f"cannot open read-only: {error}")]
    try:
        version = conn.execute("PRAGMA user_version").fetchone()[0]
        if version > SCHEMA_VERSION:
            return [Check("database", "fail", f"schema v{version} is newer than supported v{SCHEMA_VERSION}")]
        checks = [Check("database", "pass", f"schema v{version}")]
        if version >= 3:
            report = verify_audit(conn)
            checks.append(
                Check("audit-chain", "pass", f"{report.checked} chained event(s), {report.legacy} legacy")
                if report.ok
                else Check("audit-chain", "fail", report.error)
            )
        else:
            checks.append(Check("audit-chain", "warn", f"schema v{version}: audit not chained until upgraded"))
        if role == "apply":
            checks.append(_original_state_check(conn, version))
        return checks
    except sqlite3.Error as error:
        return [Check("database", "fail", f"unreadable: {error}")]
    finally:
        conn.close()


def _original_state_check(conn: sqlite3.Connection, version: int) -> Check:
    if version < 2:
        return Check("original-state", "warn", "run `nordfir save-original` before apply")
    count = conn.execute(
        "SELECT COUNT(*) FROM original_states WHERE retired_at IS NULL"
    ).fetchone()[0]
    if count:
        return Check("original-state", "pass", f"{count} active recovery record(s)")
    return Check("original-state", "warn", "none stored: apply stays blocked until `nordfir save-original`")


def _confinement_checks(procfs: Path, role: str) -> list[Check]:
    try:
        text = (procfs / "self/status").read_text()
    except (OSError, UnicodeDecodeError):
        return [Check("confinement", "warn", "cannot read /proc/self/status; confinement unknown")]
    fields = {k: v.split() for k, _, v in (line.partition(":") for line in text.splitlines())}
    euid = int(fields["Uid"][1]) if fields.get("Uid") and len(fields["Uid"]) > 1 else None
    no_new_privs = fields.get("NoNewPrivs", ["?"])[0]
    caps = fields.get("CapEff", ["?"])[0]
    has_caps = caps not in ("?", "0000000000000000")
    seccomp = {"0": "off", "1": "strict", "2": "filter"}.get(fields.get("Seccomp", ["?"])[0], "unknown")

    notes = []
    if euid == 0:
        notes.append("running as root" + (": the observer needs no privileges" if role == "observe" else ""))
    if euid != 0 and has_caps:
        notes.append(f"holds capabilities (CapEff={caps})")
    if no_new_privs == "0":
        notes.append("NoNewPrivs is not set")
    facts = f"uid={euid} NoNewPrivs={no_new_privs} CapEff={caps} seccomp={seccomp}"
    if notes:
        return [Check("confinement", "warn", f"{'; '.join(notes)} [{facts}]")]
    return [Check("confinement", "pass", facts)]


def _cpufreq_checks(sysfs: Path, procfs: Path) -> list[Check]:
    snapshot = collect_hardware(sysfs, procfs)
    try:
        original_state_from(snapshot)
    except ValueError as error:
        return [Check("cpufreq", "fail", f"cannot capture a safe baseline: {error}")]
    checks = [Check("cpufreq", "pass", f"governor {snapshot.cpufreq.governor}, "
                    f"{snapshot.cpufreq.hardware_min_khz}-{snapshot.cpufreq.hardware_max_khz} kHz")]

    policies = sorted(p for p in (sysfs / "devices/system/cpu").glob("cpu[0-9]*/cpufreq") if p.is_dir())
    unwritable = [
        str(p / name)
        for p in policies
        for name in ("scaling_governor", "scaling_max_freq")
        if not os.access(p / name, os.W_OK, effective_ids=True)
    ]
    checks.append(
        Check("cpufreq-write", "fail", f"{len(unwritable)} file(s) not writable by this user, "
              f"e.g. {unwritable[0]} (writes normally need root or a udev rule)")
        if unwritable
        else Check("cpufreq-write", "pass", f"governor and max frequency writable on {len(policies)} CPU(s)")
    )
    return checks


def _pending_restore_check(sysfs: Path, procfs: Path, state: Path) -> list[Check]:
    db = state / "nordfir.db"
    if not db.exists():
        return []
    snapshot = collect_hardware(sysfs, procfs)
    try:
        conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
        try:
            if conn.execute("PRAGMA user_version").fetchone()[0] < 2:
                return []
            original = load_original_state(conn, snapshot.node)
        finally:
            conn.close()
    except sqlite3.Error:
        return []
    if original is None:
        return []
    cpu = snapshot.cpufreq
    if (cpu.governor, cpu.scaling_max_khz) != (original.governor, original.scaling_max_khz):
        return [Check("pending-restore", "warn",
                      "node differs from its stored original state; changes are in place "
                      "(`nordfir plan-restore` shows how to undo them)")]
    return []


def _readable(path: Path) -> bool:
    try:
        path.read_text()
        return True
    except (OSError, UnicodeDecodeError):
        return False
