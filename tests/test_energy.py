import shutil
from dataclasses import replace

import pytest

from nordfir.cli import main
from nordfir.db import initiate_db
from nordfir.energy import (
    GenericModel,
    cpu_utilization,
    fit_calibration,
    load_power_model,
    power_report,
    rapl_power,
    read_hwmon_power,
    read_rapl_energy,
    save_power_model,
)
from nordfir.hardware import collect_hardware


@pytest.fixture
def snap(sysfs, procfs):
    return collect_hardware(sysfs("full"), procfs, node="test")  # load 0.52, 1 cpu


@pytest.fixture
def host(tmp_path, sysfs):
    root = tmp_path / "sys"
    shutil.copytree(sysfs("full"), root)
    return root


def add_hwmon(host, value, sensor="hwmon2/power1_input"):
    path = host / "class/hwmon" / sensor
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(str(value))


# --- RAPL ---------------------------------------------------------------------

def test_reads_top_level_rapl_domains_only(host):
    sub = host / "class/powercap/intel-rapl:0/intel-rapl:0:0"
    sub.mkdir()
    (sub / "energy_uj").write_text("5")

    assert read_rapl_energy(host) == {"intel-rapl:0": (123456, None)}


def test_missing_or_unreadable_rapl_is_empty(sysfs, tmp_path):
    assert read_rapl_energy(sysfs("missing")) == {}
    assert read_rapl_energy(tmp_path / "nope") == {}


def test_rapl_power_is_a_package_measurement():
    reading = rapl_power({"d": (1_000_000, None)}, {"d": (11_000_000, None)}, seconds=2, now=1.0)

    assert reading.watts == 5.0
    assert (reading.source, reading.scope, reading.measured) == ("rapl", "package", True)


def test_rapl_counter_wrap_needs_a_known_range():
    wrapped = ({"d": (900, 1000)}, {"d": (100, 1000)})
    assert rapl_power(*wrapped, seconds=1e-6).watts == pytest.approx(200 / 1e6 / 1e-6)
    assert rapl_power({"d": (900, None)}, {"d": (100, None)}, seconds=1) is None


def test_rapl_needs_positive_time_and_shared_domains():
    assert rapl_power({"a": (1, None)}, {"a": (2, None)}, seconds=0) is None
    assert rapl_power({"a": (1, None)}, {"b": (2, None)}, seconds=1) is None


# --- hwmon --------------------------------------------------------------------

def test_hwmon_reading_carries_the_configured_scope(host):
    add_hwmon(host, 42_500_000)

    system = read_hwmon_power(host, "hwmon2/power1_input", "system")
    package = read_hwmon_power(host, "hwmon2/power1_input", "package")

    assert system.watts == 42.5 and system.scope == "system" and system.measured
    assert package.scope == "package" and package.confidence < system.confidence


@pytest.mark.parametrize("sensor", ["../../etc/passwd", "/etc/passwd", "hwmon2/../x", "hwmon/power1_input"])
def test_hwmon_sensor_path_is_validated(host, sensor):
    with pytest.raises(ValueError, match="invalid hwmon sensor"):
        read_hwmon_power(host, sensor, "system")


def test_hwmon_unreadable_or_bad_value_is_none(host):
    assert read_hwmon_power(host, "hwmon9/power1_input", "system") is None
    add_hwmon(host, "garbage")
    assert read_hwmon_power(host, "hwmon2/power1_input", "system") is None


# --- estimates ------------------------------------------------------------------

def test_utilization_is_load_per_cpu_clamped(snap, host):
    assert cpu_utilization(snap, host) == pytest.approx(0.52)
    assert cpu_utilization(replace(snap, load_1m=9.0), host) == 1.0
    assert cpu_utilization(replace(snap, load_1m=None), host) is None
    assert cpu_utilization(snap, host / "missing") is None


def test_generic_estimate_is_labelled_as_an_estimate():
    reading = GenericModel(idle_watts=30, max_watts=100).estimate(0.5)

    assert reading.watts == 65 and not reading.measured
    assert reading.source == "generic-estimate" and reading.confidence <= 0.3
    assert reading.uncertainty_watts == 40


def test_fit_recovers_a_line_and_reports_error():
    samples = [(u / 10, 40 + 60 * u / 10) for u in range(0, 11, 2)]
    model = fit_calibration(samples)

    assert model.intercept_watts == pytest.approx(40) and model.slope_watts == pytest.approx(60)
    assert model.residual_std_watts == pytest.approx(0, abs=1e-9) and model.r_squared == 1.0

    noisy = fit_calibration([(0.0, 40), (0.25, 58), (0.5, 66), (0.75, 88), (1.0, 97)])
    assert noisy.residual_std_watts > 0 and 0.9 < noisy.r_squared < 1
    reading = noisy.estimate(0.5)
    assert reading.source == "calibrated-estimate" and reading.uncertainty_watts > 0
    assert not reading.measured


@pytest.mark.parametrize("samples,message", [
    ([(0.1, 50)] * 4, "at least"),
    ([(0.5, 50)] * 6, "does not vary"),
    ([(0.0, 50), (0.2, 50), (0.4, 50), (0.6, 40), (0.8, 30), (1.0, 20)], "falls"),
    ([(0.0, 50), (0.2, 55), (0.4, 60), (0.6, 65), (1.5, 70)], "invalid sample"),
    ([(0.0, 50), (0.2, 55), (0.4, 0), (0.6, 65), (1.0, 70)], "invalid sample"),
    ([(0.0, 50), (0.2, 55), (0.4, float("nan")), (0.6, 65), (1.0, 70)], "invalid sample"),
])
def test_fit_refuses_weak_or_implausible_samples(samples, message):
    with pytest.raises(ValueError, match=message):
        fit_calibration(samples)


def test_model_round_trips_through_the_database(tmp_path):
    conn = initiate_db(tmp_path)
    model = fit_calibration([(0.0, 40), (0.25, 55), (0.5, 70), (0.75, 85), (1.0, 100)])

    assert load_power_model(conn, "test") is None
    save_power_model(conn, "test", model)
    assert load_power_model(conn, "test") == model
    newer = replace(model, intercept_watts=1.0)
    save_power_model(conn, "test", newer)
    assert load_power_model(conn, "test") == newer


# --- report -----------------------------------------------------------------------

def test_priority_is_system_sensor_then_calibration_then_generic(snap, host):
    add_hwmon(host, 50_000_000)
    model = fit_calibration([(0.0, 40), (0.25, 55), (0.5, 70), (0.75, 85), (1.0, 100)])

    assert power_report(snap, host, hwmon=("hwmon2/power1_input", "system"), model=model).system.source == "hwmon"
    assert power_report(snap, host, model=model).system.source == "calibrated-estimate"
    generic = power_report(snap, host)
    assert generic.system.source == "generic-estimate"
    assert any("generic estimate" in n for n in generic.notes)


def test_component_readings_never_become_system_power(snap, host):
    add_hwmon(host, 12_000_000)
    rapl = rapl_power({"d": (0, None)}, {"d": (5_000_000, None)}, 1)

    report = power_report(snap, host, hwmon=("hwmon2/power1_input", "package"), rapl=rapl)

    assert [c.source for c in report.components] == ["hwmon", "rapl"]
    assert report.system.source == "generic-estimate"  # fell back; did not promote 12 W
    assert any("partial" in n for n in report.notes)


def test_unreadable_sensor_falls_back_with_a_note(snap, host):
    report = power_report(snap, host, hwmon=("hwmon7/power1_input", "system"))

    assert report.system.source == "generic-estimate"
    assert any("unreadable" in n for n in report.notes)


def test_no_estimate_when_utilization_unknown(snap, host):
    report = power_report(replace(snap, load_1m=None), host)

    assert report.system is None and any("unknown" in n for n in report.notes)


# --- CLI ---------------------------------------------------------------------------

def test_cli_calibrate_then_power(tmp_path, host, procfs, capsys):
    base = ["--sysfs-root", str(host), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path / "state")]
    csv = tmp_path / "samples.csv"
    csv.write_text("utilization,watts\n0,40\n0.25,55\n0.5,70\n0.75,85\n1,100\n")

    assert main(base + ["power", "--sample-seconds", "0", *state]) == 0
    assert "generic-estimate" in capsys.readouterr().out

    assert main(base + ["calibrate", str(csv), *state]) == 0
    assert main(base + ["power", "--sample-seconds", "0", *state]) == 0
    out = capsys.readouterr().out
    assert "calibrated-estimate" in out and '"measured": false' in out


def test_cli_calibrate_refuses_bad_input(tmp_path, host, procfs):
    base = ["--sysfs-root", str(host), "--procfs-root", str(procfs)]
    state = ["--state-dir", str(tmp_path / "state")]
    bad = tmp_path / "bad.csv"
    bad.write_text("utilization,watts\n0.1,50\n")

    assert main(base + ["calibrate", str(bad), *state]) == 1
    assert main(base + ["calibrate", str(tmp_path / "missing.csv"), *state]) == 1
    bad.write_text("x,y\n1,2\n")
    assert main(base + ["calibrate", str(bad), *state]) == 1


def test_cli_power_with_hwmon_and_rapl(tmp_path, host, procfs, capsys):
    add_hwmon(host, 61_000_000)
    base = ["--sysfs-root", str(host), "--procfs-root", str(procfs)]
    args = ["power", "--hwmon", "hwmon2/power1_input", "--hwmon-scope", "system",
            "--sample-seconds", "0.01", "--state-dir", str(tmp_path / "state")]

    assert main(base + args) == 0
    out = capsys.readouterr().out
    assert '"source": "hwmon"' in out and '"source": "rapl"' in out

    assert main(base + ["power", "--hwmon", "../../x", "--state-dir", str(tmp_path / "state")]) == 1
