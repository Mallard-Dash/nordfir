use std::{fs, path::PathBuf};

use crate::energy::PowerCapabilities;

/// Read-only discovery of Linux power-management interfaces.
///
/// The sysfs root is injectable so tests never inspect or depend on the host's
/// real `/sys` tree.
#[derive(Debug, Clone)]
pub struct LinuxPowerProbe {
    sysfs_root: PathBuf,
}

impl Default for LinuxPowerProbe {
    fn default() -> Self {
        Self::new("/sys")
    }
}

impl LinuxPowerProbe {
    pub fn new(sysfs_root: impl Into<PathBuf>) -> Self {
        Self {
            sysfs_root: sysfs_root.into(),
        }
    }

    pub fn probe(&self) -> PowerCapabilities {
        let cpufreq = self
            .sysfs_root
            .join("devices/system/cpu/cpu0/cpufreq");
        let governor_path = cpufreq.join("scaling_governor");

        PowerCapabilities {
            cpufreq_available: cpufreq.is_dir(),
            current_governor: read_trimmed(governor_path.clone()),
            available_governors: read_trimmed(cpufreq.join("scaling_available_governors"))
                .map(|value| value.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
            hardware_min_mhz: read_frequency_mhz(cpufreq.join("cpuinfo_min_freq")),
            hardware_max_mhz: read_frequency_mhz(cpufreq.join("cpuinfo_max_freq")),
            scaling_min_mhz: read_frequency_mhz(cpufreq.join("scaling_min_freq")),
            scaling_max_mhz: read_frequency_mhz(cpufreq.join("scaling_max_freq")),
            rapl_available: self.rapl_available(),
            control_writable: governor_path
                .metadata()
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false),
        }
    }

    fn rapl_available(&self) -> bool {
        let powercap = self.sysfs_root.join("class/powercap");
        let Ok(entries) = fs::read_dir(powercap) else {
            return false;
        };
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("rapl"))
    }
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn read_frequency_mhz(path: PathBuf) -> Option<f32> {
    read_trimmed(path)?
        .parse::<f32>()
        .ok()
        .map(|khz| khz / 1000.0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sysfs")
            .join(name)
    }

    #[test]
    fn discovers_complete_power_capabilities() {
        let capabilities = LinuxPowerProbe::new(fixture("full")).probe();

        assert!(capabilities.cpufreq_available);
        assert_eq!(
            capabilities.current_governor.as_deref(),
            Some("powersave")
        );
        assert_eq!(
            capabilities.available_governors,
            vec!["performance", "powersave"]
        );
        assert_eq!(capabilities.hardware_min_mhz, Some(400.0));
        assert_eq!(capabilities.hardware_max_mhz, Some(4700.0));
        assert_eq!(capabilities.scaling_min_mhz, Some(400.0));
        assert_eq!(capabilities.scaling_max_mhz, Some(2800.0));
        assert!(capabilities.rapl_available);
        assert!(capabilities.control_writable);
    }

    #[test]
    fn missing_cpufreq_is_reported_without_failing() {
        let capabilities = LinuxPowerProbe::new(fixture("missing")).probe();

        assert_eq!(capabilities, PowerCapabilities::unavailable());
    }

    #[test]
    fn invalid_frequency_values_remain_unknown() {
        let capabilities = LinuxPowerProbe::new(fixture("invalid")).probe();

        assert!(capabilities.cpufreq_available);
        assert_eq!(
            capabilities.current_governor.as_deref(),
            Some("performance")
        );
        assert_eq!(capabilities.hardware_min_mhz, None);
        assert_eq!(capabilities.hardware_max_mhz, None);
    }
}
