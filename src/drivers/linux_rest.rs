use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    energy::{PowerCapabilities, RestChange, RestChangePlan, RestPlanStatus},
    state::OriginalPowerState,
};

const CPUFREQ_PATH: &str = "devices/system/cpu/cpu0/cpufreq";

/// Result of an explicitly authorized Linux REST transition.
#[derive(Debug, Clone, PartialEq)]
pub struct RestApplyReport {
    pub applied: Vec<RestChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoredSetting {
    CpuMaximumFrequency,
    CpuMinimumFrequency,
    CpuGovernor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRestoreReport {
    pub restored: Vec<RestoredSetting>,
}

#[derive(Debug, Clone)]
struct LivePowerState {
    governor: String,
    scaling_min_mhz: f32,
    scaling_max_mhz: f32,
}

/// Applies only pre-planned, bounded cpufreq changes under an injectable sysfs root.
#[derive(Debug, Clone)]
pub struct LinuxRestDriver {
    sysfs_root: PathBuf,
}

impl Default for LinuxRestDriver {
    fn default() -> Self {
        Self::new("/sys")
    }
}

impl LinuxRestDriver {
    pub fn new(sysfs_root: impl Into<PathBuf>) -> Self {
        Self {
            sysfs_root: sysfs_root.into(),
        }
    }

    pub fn apply(
        &self,
        plan: &RestChangePlan,
        original: &OriginalPowerState,
    ) -> Result<RestApplyReport, String> {
        match plan.status {
            RestPlanStatus::Blocked => {
                return Err("refusing to apply a blocked REST plan".to_owned());
            }
            RestPlanStatus::NoChanges => {
                return Ok(RestApplyReport {
                    applied: Vec::new(),
                });
            }
            RestPlanStatus::Ready => {}
        }

        Self::validate_plan(plan, original)?;
        Self::verify_original_matches_plan(plan, original)?;
        self.verify_live_matches_plan(plan)?;
        let mut applied = Vec::new();

        for change in &plan.changes {
            if let Err(error) = self.apply_change(change) {
                let mut possibly_applied = applied.clone();
                possibly_applied.push(change.clone());
                let rollback = self.rollback(&possibly_applied, original);
                return match rollback {
                    Ok(()) => Err(format!("REST apply failed and was rolled back: {error}")),
                    Err(rollback_error) => Err(format!(
                        "REST apply failed: {error}; rollback also failed: {rollback_error}"
                    )),
                };
            }
            applied.push(change.clone());
        }

        Ok(RestApplyReport { applied })
    }

    pub fn restore_active(
        &self,
        original: &OriginalPowerState,
        capabilities: &PowerCapabilities,
    ) -> Result<ActiveRestoreReport, String> {
        Self::validate_restore_target(original, capabilities)?;
        let live = self.read_live_state()?;
        let mut restored = Vec::new();

        let steps = [
            RestoredSetting::CpuMaximumFrequency,
            RestoredSetting::CpuMinimumFrequency,
            RestoredSetting::CpuGovernor,
        ];
        for setting in steps {
            if Self::setting_matches(setting, original, &live) {
                continue;
            }
            if let Err(error) = self.restore_setting(setting, original) {
                let mut possibly_restored = restored.clone();
                possibly_restored.push(setting);
                let rollback = self.rollback_restore(&possibly_restored, &live);
                return match rollback {
                    Ok(()) => Err(format!(
                        "ACTIVE restore failed and was rolled back: {error}"
                    )),
                    Err(rollback_error) => Err(format!(
                        "ACTIVE restore failed: {error}; rollback also failed: {rollback_error}"
                    )),
                };
            }
            restored.push(setting);
        }

        Ok(ActiveRestoreReport { restored })
    }

    fn validate_restore_target(
        original: &OriginalPowerState,
        capabilities: &PowerCapabilities,
    ) -> Result<(), String> {
        if !capabilities.cpufreq_available {
            return Err("cannot restore ACTIVE state without Linux cpufreq".to_owned());
        }
        if !capabilities
            .available_governors
            .iter()
            .any(|governor| governor == &original.governor)
        {
            return Err("saved original governor is no longer available".to_owned());
        }
        let (Some(hardware_min), Some(hardware_max)) =
            (capabilities.hardware_min_mhz, capabilities.hardware_max_mhz)
        else {
            return Err("cannot validate saved frequencies against hardware limits".to_owned());
        };
        if original.scaling_min_mhz < hardware_min
            || original.scaling_max_mhz > hardware_max
            || original.scaling_min_mhz > original.scaling_max_mhz
        {
            return Err("saved original frequencies are outside hardware limits".to_owned());
        }
        Ok(())
    }

    fn read_live_state(&self) -> Result<LivePowerState, String> {
        Ok(LivePowerState {
            governor: read_trimmed(&self.cpufreq_path("scaling_governor"))?,
            scaling_min_mhz: read_frequency(&self.cpufreq_path("scaling_min_freq"))?,
            scaling_max_mhz: read_frequency(&self.cpufreq_path("scaling_max_freq"))?,
        })
    }

    fn setting_matches(
        setting: RestoredSetting,
        original: &OriginalPowerState,
        live: &LivePowerState,
    ) -> bool {
        match setting {
            RestoredSetting::CpuMaximumFrequency => {
                approximately_equal(live.scaling_max_mhz, original.scaling_max_mhz)
            }
            RestoredSetting::CpuMinimumFrequency => {
                approximately_equal(live.scaling_min_mhz, original.scaling_min_mhz)
            }
            RestoredSetting::CpuGovernor => live.governor == original.governor,
        }
    }

    fn restore_setting(
        &self,
        setting: RestoredSetting,
        original: &OriginalPowerState,
    ) -> Result<(), String> {
        match setting {
            RestoredSetting::CpuMaximumFrequency => write_and_verify_frequency(
                &self.cpufreq_path("scaling_max_freq"),
                original.scaling_max_mhz,
            ),
            RestoredSetting::CpuMinimumFrequency => write_and_verify_frequency(
                &self.cpufreq_path("scaling_min_freq"),
                original.scaling_min_mhz,
            ),
            RestoredSetting::CpuGovernor => {
                write_and_verify_text(&self.cpufreq_path("scaling_governor"), &original.governor)
            }
        }
    }

    fn rollback_restore(
        &self,
        restored: &[RestoredSetting],
        live: &LivePowerState,
    ) -> Result<(), String> {
        let mut errors = Vec::new();
        for setting in restored.iter().rev() {
            let result = match setting {
                RestoredSetting::CpuMaximumFrequency => write_and_verify_frequency(
                    &self.cpufreq_path("scaling_max_freq"),
                    live.scaling_max_mhz,
                ),
                RestoredSetting::CpuMinimumFrequency => write_and_verify_frequency(
                    &self.cpufreq_path("scaling_min_freq"),
                    live.scaling_min_mhz,
                ),
                RestoredSetting::CpuGovernor => {
                    write_and_verify_text(&self.cpufreq_path("scaling_governor"), &live.governor)
                }
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn validate_plan(plan: &RestChangePlan, original: &OriginalPowerState) -> Result<(), String> {
        if !plan.blockers.is_empty() || plan.changes.is_empty() {
            return Err("ready REST plan is internally inconsistent".to_owned());
        }

        let mut governor_seen = false;
        let mut frequency_seen = false;
        for change in &plan.changes {
            match change {
                RestChange::CpuGovernor { target, .. } => {
                    if governor_seen || target != "powersave" {
                        return Err("REST plan contains an invalid governor change".to_owned());
                    }
                    governor_seen = true;
                }
                RestChange::CpuMaxFrequencyMhz { current, target } => {
                    if frequency_seen
                        || !current.is_finite()
                        || !target.is_finite()
                        || *target < original.scaling_min_mhz
                        || *target > *current
                    {
                        return Err("REST plan contains an invalid frequency change".to_owned());
                    }
                    frequency_seen = true;
                }
            }
        }
        Ok(())
    }

    fn verify_live_matches_plan(&self, plan: &RestChangePlan) -> Result<(), String> {
        for change in &plan.changes {
            match change {
                RestChange::CpuGovernor { current, .. } => {
                    verify_text(&self.cpufreq_path("scaling_governor"), current)?;
                }
                RestChange::CpuMaxFrequencyMhz { current, .. } => {
                    verify_frequency(&self.cpufreq_path("scaling_max_freq"), *current)?;
                }
            }
        }
        Ok(())
    }

    fn verify_original_matches_plan(
        plan: &RestChangePlan,
        original: &OriginalPowerState,
    ) -> Result<(), String> {
        for change in &plan.changes {
            match change {
                RestChange::CpuGovernor { current, .. } if current != &original.governor => {
                    return Err("REST plan governor does not match saved original state".to_owned());
                }
                RestChange::CpuMaxFrequencyMhz { current, .. }
                    if !approximately_equal(*current, original.scaling_max_mhz) =>
                {
                    return Err(
                        "REST plan maximum frequency does not match saved original state"
                            .to_owned(),
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn apply_change(&self, change: &RestChange) -> Result<(), String> {
        match change {
            RestChange::CpuGovernor { current, target } => {
                let path = self.cpufreq_path("scaling_governor");
                verify_text(&path, current)?;
                write_and_verify_text(&path, target)
            }
            RestChange::CpuMaxFrequencyMhz { current, target } => {
                let path = self.cpufreq_path("scaling_max_freq");
                verify_frequency(&path, *current)?;
                write_and_verify_frequency(&path, *target)
            }
        }
    }

    fn rollback(
        &self,
        applied: &[RestChange],
        original: &OriginalPowerState,
    ) -> Result<(), String> {
        let mut errors = Vec::new();
        for change in applied.iter().rev() {
            let result = match change {
                RestChange::CpuGovernor { .. } => write_and_verify_text(
                    &self.cpufreq_path("scaling_governor"),
                    &original.governor,
                ),
                RestChange::CpuMaxFrequencyMhz { .. } => write_and_verify_frequency(
                    &self.cpufreq_path("scaling_max_freq"),
                    original.scaling_max_mhz,
                ),
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn cpufreq_path(&self, name: &str) -> PathBuf {
        self.sysfs_root.join(CPUFREQ_PATH).join(name)
    }
}

fn read_trimmed(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_owned())
        .map_err(|error| format!("read {}: {error}", path.display()))
}

fn verify_text(path: &Path, expected: &str) -> Result<(), String> {
    let actual = read_trimmed(path)?;
    if actual != expected {
        return Err(format!(
            "{} changed since planning: expected '{expected}', found '{actual}'",
            path.display()
        ));
    }
    Ok(())
}

fn write_and_verify_text(path: &Path, value: &str) -> Result<(), String> {
    fs::write(path, value).map_err(|error| format!("write {}: {error}", path.display()))?;
    verify_text(path, value)
}

fn verify_frequency(path: &Path, expected_mhz: f32) -> Result<(), String> {
    let actual_mhz = read_frequency(path)?;
    if !approximately_equal(actual_mhz, expected_mhz) {
        return Err(format!(
            "{} changed since planning: expected {expected_mhz:.0} MHz, found {actual_mhz:.0} MHz",
            path.display()
        ));
    }
    Ok(())
}

fn read_frequency(path: &Path) -> Result<f32, String> {
    let actual_khz = read_trimmed(path)?
        .parse::<f32>()
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    Ok(actual_khz / 1000.0)
}

fn write_and_verify_frequency(path: &Path, value_mhz: f32) -> Result<(), String> {
    if !value_mhz.is_finite() || value_mhz <= 0.0 {
        return Err("refusing to write an invalid CPU frequency".to_owned());
    }
    let value_khz = (value_mhz * 1000.0).round() as u64;
    fs::write(path, value_khz.to_string())
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    verify_frequency(path, value_mhz)
}

fn approximately_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, UNIX_EPOCH},
    };

    use crate::{core::NodeId, energy::PowerCapabilities};

    use super::*;

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "nordfir-rest-driver-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let cpufreq = root.join(CPUFREQ_PATH);
        fs::create_dir_all(&cpufreq).expect("fixture directory should be created");
        fs::write(cpufreq.join("scaling_governor"), "performance")
            .expect("governor fixture should be written");
        fs::write(cpufreq.join("scaling_max_freq"), "4700000")
            .expect("frequency fixture should be written");
        fs::write(cpufreq.join("scaling_min_freq"), "400000")
            .expect("frequency fixture should be written");
        (root, cpufreq)
    }

    fn original() -> OriginalPowerState {
        let capabilities = PowerCapabilities {
            cpufreq_available: true,
            current_governor: Some("performance".to_owned()),
            available_governors: vec!["performance".to_owned(), "powersave".to_owned()],
            hardware_min_mhz: Some(400.0),
            hardware_max_mhz: Some(4700.0),
            scaling_min_mhz: Some(400.0),
            scaling_max_mhz: Some(4700.0),
            rapl_available: false,
            control_writable: true,
        };
        OriginalPowerState::capture(
            NodeId::new("local"),
            &capabilities,
            UNIX_EPOCH + Duration::from_secs(1234),
        )
        .expect("original state should be valid")
    }

    fn plan() -> RestChangePlan {
        RestChangePlan {
            status: RestPlanStatus::Ready,
            changes: vec![
                RestChange::CpuGovernor {
                    current: "performance".to_owned(),
                    target: "powersave".to_owned(),
                },
                RestChange::CpuMaxFrequencyMhz {
                    current: 4700.0,
                    target: 1880.0,
                },
            ],
            blockers: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn applies_and_verifies_a_ready_plan() {
        let (root, cpufreq) = fixture();
        let report = LinuxRestDriver::new(&root)
            .apply(&plan(), &original())
            .expect("valid plan should apply");

        assert_eq!(report.applied.len(), 2);
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_governor")).unwrap(),
            "powersave"
        );
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_max_freq")).unwrap(),
            "1880000"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_a_stale_plan_before_writing() {
        let (root, cpufreq) = fixture();
        fs::write(cpufreq.join("scaling_governor"), "schedutil").unwrap();

        let result = LinuxRestDriver::new(&root).apply(&plan(), &original());

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_governor")).unwrap(),
            "schedutil"
        );
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_max_freq")).unwrap(),
            "4700000"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_write_when_a_preflight_interface_is_missing() {
        let (root, cpufreq) = fixture();
        fs::remove_file(cpufreq.join("scaling_max_freq")).unwrap();

        let result = LinuxRestDriver::new(&root).apply(&plan(), &original());

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_governor")).unwrap(),
            "performance"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_restores_completed_changes() {
        let (root, cpufreq) = fixture();
        fs::write(cpufreq.join("scaling_governor"), "powersave").unwrap();
        fs::write(cpufreq.join("scaling_max_freq"), "1880000").unwrap();
        let driver = LinuxRestDriver::new(&root);

        driver
            .rollback(&plan().changes, &original())
            .expect("rollback should restore both values");

        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_governor")).unwrap(),
            "performance"
        );
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_max_freq")).unwrap(),
            "4700000"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_blocked_plans() {
        let (root, _) = fixture();
        let mut blocked = plan();
        blocked.status = RestPlanStatus::Blocked;

        assert!(
            LinuxRestDriver::new(&root)
                .apply(&blocked, &original())
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restores_original_active_settings_in_safe_order() {
        let (root, cpufreq) = fixture();
        fs::write(cpufreq.join("scaling_governor"), "powersave").unwrap();
        fs::write(cpufreq.join("scaling_max_freq"), "1880000").unwrap();

        let report = LinuxRestDriver::new(&root)
            .restore_active(&original(), &capabilities())
            .expect("original settings should restore");

        assert_eq!(
            report.restored,
            vec![
                RestoredSetting::CpuMaximumFrequency,
                RestoredSetting::CpuGovernor
            ]
        );
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_max_freq")).unwrap(),
            "4700000"
        );
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_governor")).unwrap(),
            "performance"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restore_rejects_original_values_outside_hardware_limits() {
        let (root, cpufreq) = fixture();
        let mut invalid = original();
        invalid.scaling_max_mhz = 5000.0;

        let result = LinuxRestDriver::new(&root).restore_active(&invalid, &capabilities());

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(cpufreq.join("scaling_max_freq")).unwrap(),
            "4700000"
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn capabilities() -> PowerCapabilities {
        PowerCapabilities {
            cpufreq_available: true,
            current_governor: Some("powersave".to_owned()),
            available_governors: vec!["performance".to_owned(), "powersave".to_owned()],
            hardware_min_mhz: Some(400.0),
            hardware_max_mhz: Some(4700.0),
            scaling_min_mhz: Some(400.0),
            scaling_max_mhz: Some(1880.0),
            rapl_available: false,
            control_writable: true,
        }
    }
}
