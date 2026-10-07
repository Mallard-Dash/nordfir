import pytest

from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.hardware import collect_hardware
from nordfir.state import load_original_state, save_original_state


def test_save_and_load_round_trip(tmp_path, sysfs, procfs):
    conn = initiate_db(tmp_path)
    snap = collect_hardware(sysfs("full"), procfs, node="test")

    saved = save_original_state(conn, snap)

    assert saved.governor == "powersave"
    assert load_original_state(conn, "test") == saved
    assert load_original_state(conn, "other") is None


def test_never_overwrites_existing_state(tmp_path, sysfs, procfs):
    conn = initiate_db(tmp_path)
    snap = collect_hardware(sysfs("full"), procfs, node="test")
    save_original_state(conn, snap)

    with pytest.raises(RuntimeError, match="refusing to overwrite"):
        save_original_state(conn, snap)


@pytest.mark.parametrize("fixture", ["missing", "invalid"])
def test_refuses_unknown_state(tmp_path, sysfs, procfs, fixture):
    conn = initiate_db(tmp_path)
    snap = collect_hardware(sysfs(fixture), procfs, node="test")

    with pytest.raises(ValueError):
        save_original_state(conn, snap)
    assert load_original_state(conn, "test") is None


def test_cli_save_and_show(tmp_path, sysfs, procfs, capsys):
    base = ["--sysfs-root", str(sysfs("full")), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path)]

    assert main(base + ["save-original", *state]) == 0
    assert main(base + ["save-original", *state]) == 1  # second save refused
    capsys.readouterr()

    node = collect_hardware(sysfs("full"), procfs).node
    assert main(["show-original", *state, "--node", node]) == 0
    assert '"governor": "powersave"' in capsys.readouterr().out
