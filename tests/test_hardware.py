from nordfir.hardware import collect_hardware


def test_full_fixture_reads_cpufreq_and_rapl(sysfs, procfs):
    snap = collect_hardware(sysfs("full"), procfs, node="test")

    assert snap.cpufreq_available
    assert snap.rapl_available
    assert snap.cpufreq.governor == "powersave"
    assert snap.cpufreq.available_governors == ("performance", "powersave")
    assert snap.cpufreq.hardware_max_khz == 4_700_000
    assert snap.cpufreq.scaling_max_khz == 2_800_000
    assert snap.memory_used_percent == 50.0
    assert snap.load_1m == 0.52
    assert snap.uptime_seconds == 12345.67


def test_invalid_values_become_unknown(sysfs, tmp_path):
    snap = collect_hardware(sysfs("invalid"), tmp_path, node="test")

    assert snap.cpufreq_available
    assert snap.cpufreq.governor == "performance"
    assert snap.cpufreq.hardware_min_khz is None
    assert snap.cpufreq.hardware_max_khz is None
    assert snap.memory_used_percent is None


def test_missing_interfaces_are_unavailable(sysfs, tmp_path):
    snap = collect_hardware(sysfs("missing"), tmp_path, node="test")

    assert not snap.cpufreq_available
    assert not snap.rapl_available
    assert snap.cpufreq.governor is None
