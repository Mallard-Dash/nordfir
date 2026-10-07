"""Command-line entry point.

Only ``apply`` and ``restore`` (with their confirmation flag) write to the host.
"""

from __future__ import annotations

import argparse
import csv
import dataclasses
import json
import math
import signal
import socket
import sys
import threading
import time

from nordfir.audit import verify_audit
from nordfir.db import initiate_db, open_existing_db
from nordfir.driver import DryRunDriver, LinuxCpufreqDriver
from nordfir.energy import (
    fit_calibration, load_power_model, power_report, rapl_power, read_rapl_energy,
    save_power_model,
)
from nordfir.engine import desired_state, execute_plan, restore_original, run_once
from nordfir.guards import collect_activity, guard_plan
from nordfir.hardware import collect_hardware
from nordfir.model import Intent
from nordfir.observer import observe
from nordfir.plan import plan_changes, plan_restore
from nordfir.preflight import ROLES, run_preflight
from nordfir.state import OriginalState, load_original_state, save_original_state

DEFAULT_STATE_DIR = "nordfir-state"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="nordfir", description=__doc__)
    parser.add_argument("--sysfs-root", default="/sys")
    parser.add_argument("--procfs-root", default="/proc")
    sub = parser.add_subparsers(dest="command", required=True)

    init = sub.add_parser("init-db", help="create the local state database")
    init.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    sub.add_parser("inspect", help="print the current hardware snapshot")

    plan = sub.add_parser("plan", help="print the desired state for an intent")
    plan.add_argument("intent", choices=[i.value for i in Intent])
    plan.add_argument("--record", action="store_true", help="save snapshot and decision")
    plan.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    changes = sub.add_parser("plan-changes", help="print the typed actions for an intent")
    changes.add_argument("intent", choices=[i.value for i in Intent])
    changes.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    dry = sub.add_parser(
        "dry-run", help="plan an intent and record what would be done; changes nothing"
    )
    dry.add_argument("intent", choices=[i.value for i in Intent])
    dry.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    _add_guard_options(dry)

    apply = sub.add_parser(
        "apply", help="WRITES to the host: plan an intent and apply it via cpufreq"
    )
    apply.add_argument("intent", choices=[i.value for i in Intent])
    apply.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    _add_guard_options(apply)
    apply.add_argument(
        "--confirm-system-power-write", action="store_true",
        help="required: acknowledge that this changes the host's CPU power settings",
    )

    plan_rest = sub.add_parser(
        "plan-restore", help="print the actions that would restore the original state"
    )
    plan_rest.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    restore = sub.add_parser(
        "restore", help="WRITES to the host: return to the stored original state"
    )
    restore.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    restore.add_argument("--confirm-system-power-write", action="store_true")

    watch = sub.add_parser(
        "observe", help="read-only loop: record snapshots and observations until stopped"
    )
    watch.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    watch.add_argument("--interval", type=float, default=60.0, help="seconds between cycles")
    watch.add_argument("--iterations", type=int, help="stop after N cycles (default: run until signalled)")
    watch.add_argument("--keep-snapshots", type=int, default=1000, help="snapshots to retain")
    _add_guard_options(watch)

    power = sub.add_parser("power", help="print the best power figure with its source and error")
    power.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    power.add_argument("--hwmon", metavar="SENSOR", help="e.g. hwmon2/power1_input")
    power.add_argument("--hwmon-scope", choices=["system", "package"], default="package",
                       help="what the sensor measures (default: package, i.e. not whole-system)")
    power.add_argument("--sample-seconds", type=float, default=1.0,
                       help="RAPL sampling window (0 disables RAPL)")

    calibrate = sub.add_parser("calibrate", help="fit a power model from wall-power samples")
    calibrate.add_argument("csv", help="CSV with columns: utilization (0-1), watts")
    calibrate.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    pre = sub.add_parser("preflight", help="read-only check that this host is ready")
    pre.add_argument("--role", choices=ROLES, default="apply")
    pre.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    verify = sub.add_parser("audit-verify", help="check the audit log's hash chain")
    verify.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    save = sub.add_parser(
        "save-original", help="store the current cpufreq state as recovery state"
    )
    save.add_argument("--state-dir", default=DEFAULT_STATE_DIR)

    show = sub.add_parser("show-original", help="print the stored recovery state")
    show.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    show.add_argument("--node", help="defaults to this host's name")

    args = parser.parse_args(argv)
    roots = {"sysfs_root": args.sysfs_root, "procfs_root": args.procfs_root}

    if args.command == "init-db":
        initiate_db(args.state_dir).close()
        print(f"Database ready in {args.state_dir}/")
        return 0

    if args.command == "inspect":
        _print(collect_hardware(**roots))
        return 0

    if args.command == "plan-changes":
        return _plan_changes(args.state_dir, Intent(args.intent), roots)

    if args.command == "dry-run":
        return _apply(args.state_dir, Intent(args.intent), roots, DryRunDriver(), args.protect)

    if args.command == "apply":
        if not args.confirm_system_power_write:
            print("Refused: pass --confirm-system-power-write to change the host",
                  file=sys.stderr)
            return 1
        driver = LinuxCpufreqDriver(args.sysfs_root, confirmed=True)
        return _apply(args.state_dir, Intent(args.intent), roots, driver, args.protect)

    if args.command == "plan-restore":
        return _plan_restore(args.state_dir, roots)

    if args.command == "restore":
        if not args.confirm_system_power_write:
            print("Refused: pass --confirm-system-power-write to change the host",
                  file=sys.stderr)
            return 1
        driver = LinuxCpufreqDriver(args.sysfs_root, confirmed=True)
        return _restore(args.state_dir, roots, driver)

    if args.command == "observe":
        return _observe(args, roots)

    if args.command == "power":
        return _power(args, roots)

    if args.command == "calibrate":
        return _calibrate(args, roots)

    if args.command == "preflight":
        report = run_preflight(args.role, state_dir=args.state_dir, **roots)
        _print(report)
        return 0 if report.ready else 1

    if args.command == "audit-verify":
        return _audit_verify(args.state_dir)

    if args.command == "save-original":
        return _save_original(args.state_dir, roots)

    if args.command == "show-original":
        return _show_original(args.state_dir, args.node)

    intent = Intent(args.intent)
    if args.record:
        conn = initiate_db(args.state_dir)
        try:
            desired = run_once(conn, intent, **roots)
        finally:
            conn.close()
    else:
        desired = desired_state(collect_hardware(**roots), intent)
    _print(desired)
    print("Apply: false")
    return 1 if desired.blocked else 0


def _plan_changes(state_dir: str, intent: Intent, roots: dict) -> int:
    snapshot = collect_hardware(**roots)
    original = _stored_original(state_dir, snapshot.node)
    plan = plan_changes(snapshot, desired_state(snapshot, intent), original)
    _print(plan)
    print("Apply: false")
    return 1 if plan.blocked else 0


def _stored_original(state_dir: str, node: str) -> OriginalState | None:
    """The active recovery record, without creating a state directory."""
    conn = open_existing_db(state_dir)
    if conn is None:
        return None
    try:
        return load_original_state(conn, node)
    finally:
        conn.close()


def _add_guard_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--protect", action="append", default=[], metavar="PROCESS",
        help="process name that defers changes while running (repeatable)",
    )


def _apply(state_dir: str, intent: Intent, roots: dict, driver, protect: list[str]) -> int:
    snapshot = collect_hardware(**roots)
    conn = initiate_db(state_dir)
    try:
        original = load_original_state(conn, snapshot.node)
        plan = plan_changes(snapshot, desired_state(snapshot, intent), original)
        activity = collect_activity(roots["procfs_root"], tuple(protect))
        plan = guard_plan(plan, snapshot, activity)
        result = execute_plan(conn, driver, plan)
    finally:
        conn.close()
    _print(plan if result is None else result)
    print(f"Apply: {str(result is not None and result.applied).lower()}")
    return 1 if result is None or result.error else 0


def _plan_restore(state_dir: str, roots: dict) -> int:
    snapshot = collect_hardware(**roots)
    plan = plan_restore(snapshot, _stored_original(state_dir, snapshot.node))
    _print(plan)
    print("Apply: false")
    return 1 if plan.blocked else 0


def _restore(state_dir: str, roots: dict, driver) -> int:
    snapshot = collect_hardware(**roots)
    conn = initiate_db(state_dir)
    try:
        plan, result = restore_original(conn, driver, snapshot)
    finally:
        conn.close()
    _print(plan if result is None else result)
    print(f"Apply: {str(result is not None and result.applied).lower()}")
    return 1 if result is None or result.error else 0


def _observe(args: argparse.Namespace, roots: dict) -> int:
    if (
        not (args.interval > 0 and math.isfinite(args.interval))
        or args.keep_snapshots < 1
        or (args.iterations is not None and args.iterations < 1)
    ):
        print("Refused: --interval must be positive and finite; "
              "--iterations and --keep-snapshots at least 1", file=sys.stderr)
        return 1
    stop = threading.Event()
    handlers = {
        sig: signal.signal(sig, lambda *_: stop.set())
        for sig in (signal.SIGINT, signal.SIGTERM)
    }
    conn = None
    try:
        conn = initiate_db(args.state_dir)
        summary = observe(
            conn, interval=args.interval, stop=stop, iterations=args.iterations,
            protect=tuple(args.protect), keep_snapshots=args.keep_snapshots, **roots,
        )
    finally:
        if conn is not None:
            conn.close()
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
    _print(summary)
    return 1 if summary.reason == "too many errors" else 0


def _power(args: argparse.Namespace, roots: dict) -> int:
    snapshot = collect_hardware(**roots)
    sysfs = roots["sysfs_root"]
    rapl = None
    if args.sample_seconds > 0:
        before = read_rapl_energy(sysfs)
        time.sleep(args.sample_seconds)
        rapl = rapl_power(before, read_rapl_energy(sysfs), args.sample_seconds)
    model = None
    conn = open_existing_db(args.state_dir)
    if conn is not None:
        try:
            model = load_power_model(conn, snapshot.node)
        finally:
            conn.close()
    try:
        hwmon = (args.hwmon, args.hwmon_scope) if args.hwmon else None
        report = power_report(snapshot, sysfs, hwmon=hwmon, model=model, rapl=rapl)
    except ValueError as error:
        print(f"Refused: {error}", file=sys.stderr)
        return 1
    _print(report)
    return 0 if report.system else 1


def _calibrate(args: argparse.Namespace, roots: dict) -> int:
    try:
        with open(args.csv, newline="") as handle:
            samples = [(float(r["utilization"]), float(r["watts"])) for r in csv.DictReader(handle)]
        model = fit_calibration(samples)
    except (OSError, KeyError, TypeError, ValueError, csv.Error) as error:
        print(f"Refused: {error}", file=sys.stderr)
        return 1
    conn = initiate_db(args.state_dir)
    try:
        save_power_model(conn, collect_hardware(**roots).node, model)
    finally:
        conn.close()
    _print(model)
    return 0


def _audit_verify(state_dir: str) -> int:
    conn = open_existing_db(state_dir)
    if conn is None:
        print(f"No database in {state_dir}/", file=sys.stderr)
        return 1
    try:
        report = verify_audit(conn)
    finally:
        conn.close()
    _print(report)
    return 0 if report.ok else 1


def _save_original(state_dir: str, roots: dict) -> int:
    conn = initiate_db(state_dir)
    try:
        state = save_original_state(conn, collect_hardware(**roots))
    except (ValueError, RuntimeError) as error:
        print(f"Refused: {error}", file=sys.stderr)
        return 1
    finally:
        conn.close()
    _print(state)
    return 0


def _show_original(state_dir: str, node: str | None) -> int:
    state = _stored_original(state_dir, node or socket.gethostname())
    if state is None:
        print("No original state stored", file=sys.stderr)
        return 1
    _print(state)
    return 0


def _print(obj: object) -> None:
    print(json.dumps(dataclasses.asdict(obj), indent=2, default=str))


if __name__ == "__main__":
    sys.exit(main())
