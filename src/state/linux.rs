use std::{collections::{BTreeMap, BTreeSet}, fs, thread, time::{Duration, SystemTime}};

use crate::core::{NodeId, NodeSnapshot, Observation, PowerMode};

/// Read-only Linux collector backed by procfs/sysfs.
///
/// It intentionally avoids shell commands. Missing kernel interfaces become
/// unknown observations instead of being guessed.
#[derive(Debug, Clone)]
pub struct LinuxStateCollector {
    cpu_sample_interval: Duration,
}

impl Default for LinuxStateCollector {
    fn default() -> Self {
        Self { cpu_sample_interval: Duration::from_millis(120) }
    }
}

impl LinuxStateCollector {
    pub fn collect(&self, node: NodeId) -> Result<NodeSnapshot, String> {
        let now = SystemTime::now();
        let cpu = self.cpu_utilization().map(|value| Observation::known(value, "procfs:/proc/stat"));
        let memory = memory_utilization().map(|value| Observation::known(value, "procfs:/proc/meminfo"));
        let uptime = uptime_seconds().map(|value| Observation::known(value, "procfs:/proc/uptime"));
        let load = load_average_1m().map(|value| Observation::known(value, "procfs:/proc/loadavg"));
        let frequency = cpu_frequency_mhz().map(|value| Observation::known(value, "sysfs:cpufreq"));

        Ok(NodeSnapshot {
            node,
            power_mode: Observation::known_at(PowerMode::Active, "local-linux", now),
            reachable: Observation::known_at(true, "local-linux", now),
            active_ssh_sessions: Observation::unknown_at(
                "SSH session inspection is not implemented by the local collector yet",
                "local-linux",
                now,
            ),
            protected_services: BTreeSet::new(),
            services: BTreeMap::new(),
            cpu_utilization_percent: cpu.ok(),
            memory_utilization_percent: memory.ok(),
            load_average_1m: load.ok(),
            uptime_seconds: uptime.ok(),
            cpu_frequency_mhz: frequency.ok(),
            power: None,
        })
    }

    fn cpu_utilization(&self) -> Result<f32, String> {
        let first = read_cpu_times()?;
        thread::sleep(self.cpu_sample_interval);
        let second = read_cpu_times()?;

        let total_delta = second.total.saturating_sub(first.total);
        let idle_delta = second.idle.saturating_sub(first.idle);
        if total_delta == 0 { return Err("CPU counters did not advance".to_owned()); }
        let busy = total_delta.saturating_sub(idle_delta);
        Ok((busy as f32 / total_delta as f32) * 100.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct CpuTimes { total: u64, idle: u64 }

fn read_cpu_times() -> Result<CpuTimes, String> {
    let content = fs::read_to_string("/proc/stat").map_err(|e| format!("read /proc/stat: {e}"))?;
    let line = content.lines().next().ok_or_else(|| "missing aggregate cpu line".to_owned())?;
    let mut values = line.split_whitespace();
    if values.next() != Some("cpu") { return Err("unexpected /proc/stat format".to_owned()); }
    let numbers: Vec<u64> = values.filter_map(|v| v.parse::<u64>().ok()).collect();
    if numbers.len() < 4 { return Err("not enough CPU counters in /proc/stat".to_owned()); }
    let idle = numbers.get(3).copied().unwrap_or(0) + numbers.get(4).copied().unwrap_or(0);
    Ok(CpuTimes { total: numbers.iter().copied().sum(), idle })
}

fn memory_utilization() -> Result<f32, String> {
    let content = fs::read_to_string("/proc/meminfo").map_err(|e| format!("read /proc/meminfo: {e}"))?;
    let mut total = None;
    let mut available = None;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") { total = parse_kib(value); }
        if let Some(value) = line.strip_prefix("MemAvailable:") { available = parse_kib(value); }
    }
    let total = total.ok_or_else(|| "MemTotal missing".to_owned())?;
    let available = available.ok_or_else(|| "MemAvailable missing".to_owned())?;
    if total == 0.0 { return Err("MemTotal was zero".to_owned()); }
    Ok(((total - available).max(0.0) / total) * 100.0)
}

fn parse_kib(value: &str) -> Option<f32> { value.split_whitespace().next()?.parse::<f32>().ok() }

fn uptime_seconds() -> Result<u64, String> {
    let content = fs::read_to_string("/proc/uptime").map_err(|e| format!("read /proc/uptime: {e}"))?;
    let seconds = content.split_whitespace().next().ok_or_else(|| "uptime value missing".to_owned())?;
    Ok(seconds.parse::<f64>().map_err(|e| format!("parse uptime: {e}"))? as u64)
}

fn load_average_1m() -> Result<f32, String> {
    let content = fs::read_to_string("/proc/loadavg").map_err(|e| format!("read /proc/loadavg: {e}"))?;
    content.split_whitespace().next().ok_or_else(|| "load average missing".to_owned())?
        .parse::<f32>().map_err(|e| format!("parse load average: {e}"))
}

fn cpu_frequency_mhz() -> Result<f32, String> {
    let paths = [
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_cur_freq",
    ];
    for path in paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(khz) = content.trim().parse::<f32>() { return Ok(khz / 1000.0); }
        }
    }
    Err("CPU frequency interface unavailable".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kib_value() {
        assert_eq!(parse_kib("       16384 kB"), Some(16384.0));
    }
}
