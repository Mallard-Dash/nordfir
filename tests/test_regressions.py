"""Regression tests for bugs found in code review."""

import signal
import threading

import pytest

from nordfir.cli import main
from nordfir.observer import observe


@pytest.fixture
def base(sysfs, procfs):
    return ["--sysfs-root", str(sysfs("full")), "--procfs-root", str(procfs)]


@pytest.mark.parametrize("content", [
    "utilization,watts\n0.1,50\n0.2\n0.3,60\n0.4,65\n0.5,70\n",  # row with a missing cell
    "utilization,watts\n0.1,50\n0.2,nan\n0.3,60\n0.4,65\n0.5,70\n",
    "",
])
def test_calibrate_refuses_malformed_csv_without_a_traceback(tmp_path, base, content):
    sample = tmp_path / "s.csv"
    sample.write_text(content)

    assert main(base + ["calibrate", str(sample), "--state-dir", str(tmp_path / "state")]) == 1


@pytest.mark.parametrize("arg", ["0", "-1"])
def test_observe_rejects_non_positive_iterations(tmp_path, base, arg):
    assert main(base + ["observe", "--iterations", arg, "--state-dir", str(tmp_path / "s")]) == 1
    assert not (tmp_path / "s").exists()


@pytest.mark.parametrize("interval", ["nan", "inf", "-5"])
def test_observe_rejects_non_finite_interval(tmp_path, base, interval):
    assert main(base + ["observe", "--interval", interval, "--state-dir", str(tmp_path / "s")]) == 1
    with pytest.raises(ValueError):
        observe(None, interval=float(interval), stop=threading.Event())


def test_observe_restores_signal_handlers_when_setup_fails(tmp_path, base):
    blocker = tmp_path / "file"
    blocker.write_text("not a directory")
    before = (signal.getsignal(signal.SIGINT), signal.getsignal(signal.SIGTERM))

    with pytest.raises(OSError):
        main(base + ["observe", "--iterations", "1", "--state-dir", str(blocker / "state")])

    assert (signal.getsignal(signal.SIGINT), signal.getsignal(signal.SIGTERM)) == before


@pytest.mark.parametrize("command", [
    ["plan-changes", "economize"],
    ["plan-restore"],
    ["show-original", "--node", "x"],
    ["audit-verify"],
    ["power", "--sample-seconds", "0"],
])
def test_read_only_commands_do_not_create_a_state_directory(tmp_path, base, command):
    state = tmp_path / "fresh"

    main(base + command + ["--state-dir", str(state)])

    assert not state.exists()


def test_commands_still_work_without_a_database(tmp_path, base, capsys):
    state = ["--state-dir", str(tmp_path / "fresh")]

    assert main(base + ["plan-changes", "economize", *state]) == 1  # blocked: no original state
    assert "save-original" in capsys.readouterr().out
    assert main(base + ["power", "--sample-seconds", "0", *state]) == 0
    assert main(base + ["audit-verify", *state]) == 1
    assert "No database" in capsys.readouterr().err
