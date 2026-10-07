import json

import pytest

from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.driver import ApplyResult, DryRunDriver
from nordfir.engine import desired_state, execute_plan
from nordfir.hardware import collect_hardware
from nordfir.model import Intent
from nordfir.plan import ChangePlan, SetScalingMax, plan_changes
from nordfir.state import original_state_from


@pytest.fixture
def plan(sysfs, procfs):
    snap = collect_hardware(sysfs("full"), procfs, node="test")
    return plan_changes(snap, desired_state(snap, Intent.ECONOMIZE), original_state_from(snap))


def _events(conn):
    return [(e, json.loads(d)) for e, d in conn.execute("SELECT event, detail FROM audit_events")]


def test_dry_run_reports_actions_without_applying(plan):
    result = DryRunDriver().apply(plan)

    assert result == ApplyResult("dry-run", False, (SetScalingMax(2_800_000, 1_880_000),))


def test_dry_run_driver_rejects_blocked_plan():
    with pytest.raises(ValueError):
        DryRunDriver().apply(ChangePlan("test", blockers=("nope",)))


def test_execute_plan_audits_the_dry_run(tmp_path, plan):
    conn = initiate_db(tmp_path)

    assert execute_plan(conn, DryRunDriver(), plan).applied is False

    started, (event, detail) = _events(conn)
    assert started[0] == "apply_started"
    assert event == "apply" and detail["applied"] is False and detail["driver"] == "dry-run"
    assert detail["actions"] == [
        {"type": "SetScalingMax", "expected_khz": 2_800_000, "target_khz": 1_880_000}
    ]


def test_execute_plan_never_calls_driver_for_blocked_plan(tmp_path):
    class Exploding:
        name = "exploding"

        def apply(self, plan):
            raise AssertionError("driver must not be called")

    conn = initiate_db(tmp_path)
    blocked = ChangePlan("test", blockers=("no original state",))

    assert execute_plan(conn, Exploding(), blocked) is None
    assert _events(conn) == [
        (
            "apply_refused",
            {"node": "test", "driver": "exploding",
             "blockers": ["no original state"], "deferrals": []},
        )
    ]


def test_cli_dry_run(tmp_path, sysfs, procfs, capsys):
    base = ["--sysfs-root", str(sysfs("full")), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path)]

    assert main(base + ["dry-run", "economize", *state]) == 1  # no original state yet
    assert main(base + ["save-original", *state]) == 0
    capsys.readouterr()
    assert main(base + ["dry-run", "economize", *state]) == 0
    assert '"applied": false' in capsys.readouterr().out

    events = [e for e, in initiate_db(tmp_path).execute("SELECT event FROM audit_events")]
    assert events == ["apply_refused", "apply_started", "apply"]
