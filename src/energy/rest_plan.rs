use crate::core::PowerMode;

use super::{PowerCapabilities, PowerProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestPlanStatus {
    Ready,
    NoChanges,
    Blocked,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RestChange {
    CpuGovernor { current: String, target: String },
    CpuMaxFrequencyMhz { current: f32, target: f32 },
}

/// A deterministic, non-executable description of a proposed REST transition.
#[derive(Debug, Clone, PartialEq)]
pub struct RestChangePlan {
    pub status: RestPlanStatus,
    pub changes: Vec<RestChange>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RestPlanner {
    target_governor: String,
}

impl Default for RestPlanner {
    fn default() -> Self {
        Self {
            target_governor: "powersave".to_owned(),
        }
    }
}

impl RestPlanner {
    pub fn plan(&self, capabilities: &PowerCapabilities, profile: &PowerProfile) -> RestChangePlan {
        let mut plan = RestChangePlan {
            status: RestPlanStatus::Blocked,
            changes: Vec::new(),
            blockers: Vec::new(),
            warnings: Vec::new(),
        };

        if profile.mode != PowerMode::Rest {
            plan.blockers
                .push("power profile does not target REST".to_owned());
        }
        if !capabilities.cpufreq_available {
            plan.blockers
                .push("Linux cpufreq interface is unavailable".to_owned());
        }

        self.plan_governor(capabilities, &mut plan);
        self.plan_frequency(capabilities, profile, &mut plan);

        if !capabilities.control_writable {
            plan.warnings.push(
                "governor control file does not expose write permission bits; apply would require separate authorization"
                    .to_owned(),
            );
        }

        plan.status = if !plan.blockers.is_empty() {
            RestPlanStatus::Blocked
        } else if plan.changes.is_empty() {
            RestPlanStatus::NoChanges
        } else {
            RestPlanStatus::Ready
        };
        plan
    }

    fn plan_governor(&self, capabilities: &PowerCapabilities, plan: &mut RestChangePlan) {
        let Some(current) = capabilities.current_governor.as_deref() else {
            plan.blockers
                .push("current CPU governor is unknown".to_owned());
            return;
        };

        if !capabilities
            .available_governors
            .iter()
            .any(|governor| governor == &self.target_governor)
        {
            plan.blockers.push(format!(
                "required CPU governor '{}' is unavailable",
                self.target_governor
            ));
            return;
        }

        if current != self.target_governor.as_str() {
            plan.changes.push(RestChange::CpuGovernor {
                current: current.to_owned(),
                target: self.target_governor.clone(),
            });
        }
    }

    fn plan_frequency(
        &self,
        capabilities: &PowerCapabilities,
        profile: &PowerProfile,
        plan: &mut RestChangePlan,
    ) {
        let Some(percent) = profile.cpu_max_percent else {
            return;
        };
        if !(1..=100).contains(&percent) {
            plan.blockers
                .push("REST CPU maximum must be between 1 and 100 percent".to_owned());
            return;
        }

        let (Some(hardware_minimum), Some(hardware_maximum), Some(current_maximum)) = (
            capabilities.hardware_min_mhz,
            capabilities.hardware_max_mhz,
            capabilities.scaling_max_mhz,
        ) else {
            plan.blockers.push(
                "CPU frequency limits are incomplete; a safe ceiling cannot be planned".to_owned(),
            );
            return;
        };

        if hardware_minimum > hardware_maximum {
            plan.blockers
                .push("CPU hardware frequency range is invalid".to_owned());
            return;
        }

        let requested = hardware_maximum * (f32::from(percent) / 100.0);
        let target = requested.clamp(hardware_minimum, hardware_maximum).round();

        // REST must never raise a ceiling that is already more restrictive.
        if current_maximum > target {
            plan.changes.push(RestChange::CpuMaxFrequencyMhz {
                current: current_maximum,
                target,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities() -> PowerCapabilities {
        PowerCapabilities {
            cpufreq_available: true,
            current_governor: Some("performance".to_owned()),
            available_governors: vec!["performance".to_owned(), "powersave".to_owned()],
            hardware_min_mhz: Some(400.0),
            hardware_max_mhz: Some(4700.0),
            scaling_min_mhz: Some(400.0),
            scaling_max_mhz: Some(4700.0),
            rapl_available: true,
            control_writable: true,
        }
    }

    #[test]
    fn plans_bounded_governor_and_frequency_changes() {
        let plan = RestPlanner::default().plan(&capabilities(), &PowerProfile::rest_default());

        assert_eq!(plan.status, RestPlanStatus::Ready);
        assert_eq!(
            plan.changes,
            vec![
                RestChange::CpuGovernor {
                    current: "performance".to_owned(),
                    target: "powersave".to_owned(),
                },
                RestChange::CpuMaxFrequencyMhz {
                    current: 4700.0,
                    target: 1880.0,
                },
            ]
        );
        assert!(plan.blockers.is_empty());
    }

    #[test]
    fn blocks_when_cpufreq_is_unavailable() {
        let plan = RestPlanner::default().plan(
            &PowerCapabilities::unavailable(),
            &PowerProfile::rest_default(),
        );

        assert_eq!(plan.status, RestPlanStatus::Blocked);
        assert!(!plan.blockers.is_empty());
        assert!(plan.changes.is_empty());
    }

    #[test]
    fn reports_no_changes_when_rest_limits_are_already_active() {
        let mut capabilities = capabilities();
        capabilities.current_governor = Some("powersave".to_owned());
        capabilities.scaling_max_mhz = Some(1800.0);

        let plan = RestPlanner::default().plan(&capabilities, &PowerProfile::rest_default());

        assert_eq!(plan.status, RestPlanStatus::NoChanges);
        assert!(plan.changes.is_empty());
    }

    #[test]
    fn never_raises_an_existing_frequency_ceiling() {
        let mut capabilities = capabilities();
        capabilities.current_governor = Some("powersave".to_owned());
        capabilities.scaling_max_mhz = Some(1200.0);

        let plan = RestPlanner::default().plan(&capabilities, &PowerProfile::rest_default());

        assert_eq!(plan.status, RestPlanStatus::NoChanges);
        assert!(plan.changes.is_empty());
    }
}
