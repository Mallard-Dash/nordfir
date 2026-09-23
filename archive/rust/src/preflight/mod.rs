use crate::{
    action::Action,
    core::{NodeId, NodeSnapshot},
    energy::{RestChangePlan, RestPlanStatus},
    guards::GuardVerdict,
};

#[derive(Debug, Clone)]
pub struct PreflightReport {
    pub node: NodeId,
    pub action: Action,
    pub refreshed_snapshot: NodeSnapshot,
    pub verdicts: Vec<GuardVerdict>,
    pub safe_to_execute: bool,
}

pub trait Preflight: Send + Sync {
    fn run(&self, action: &Action) -> Result<PreflightReport, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessStatus {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessCheck {
    pub name: String,
    pub status: ReadinessStatus,
    pub detail: String,
}

/// Aggregates read-only deployment evidence without executing an action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeploymentPreflightReport {
    pub checks: Vec<ReadinessCheck>,
}

impl DeploymentPreflightReport {
    pub fn record(&mut self, name: impl Into<String>, result: Result<String, String>) {
        let (status, detail) = match result {
            Ok(detail) => (ReadinessStatus::Ready, detail),
            Err(detail) => (ReadinessStatus::Blocked, detail),
        };
        self.checks.push(ReadinessCheck {
            name: name.into(),
            status,
            detail,
        });
    }

    pub fn is_ready(&self) -> bool {
        !self.checks.is_empty()
            && self
                .checks
                .iter()
                .all(|check| check.status == ReadinessStatus::Ready)
    }
}

pub fn verify_expected_host(expected: &str, observed: &str) -> Result<String, String> {
    if expected.is_empty() {
        return Err("expected hostname is empty".to_owned());
    }
    if observed.is_empty() {
        return Err("observed hostname is empty".to_owned());
    }
    if observed == expected {
        Ok(format!("matched '{observed}'"))
    } else {
        Err(format!("expected '{expected}' but observed '{observed}'"))
    }
}

pub fn evaluate_rest_plan(plan: &RestChangePlan, control_writable: bool) -> Result<String, String> {
    match plan.status {
        RestPlanStatus::Blocked => Err(plan.blockers.join("; ")),
        RestPlanStatus::NoChanges => Ok("ready; no changes required".to_owned()),
        RestPlanStatus::Ready if control_writable => {
            Ok(format!("ready with {} change(s)", plan.changes.len()))
        }
        RestPlanStatus::Ready => {
            Err("planned changes require writable cpufreq controls".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_report_requires_every_check_to_pass() {
        let mut report = DeploymentPreflightReport::default();
        assert!(!report.is_ready());

        report.record("host", Ok("matched".to_owned()));
        report.record("snapshot", Err("missing".to_owned()));

        assert!(!report.is_ready());
        assert_eq!(report.checks[0].status, ReadinessStatus::Ready);
        assert_eq!(report.checks[1].status, ReadinessStatus::Blocked);
    }

    #[test]
    fn deployment_report_is_ready_when_all_checks_pass() {
        let mut report = DeploymentPreflightReport::default();
        report.record("host", Ok("matched".to_owned()));
        report.record("snapshot", Ok("fresh".to_owned()));

        assert!(report.is_ready());
    }

    #[test]
    fn host_identity_requires_an_exact_match() {
        assert!(verify_expected_host("bifrost", "bifrost").is_ok());
        assert!(verify_expected_host("bifrost", "oden").is_err());
        assert!(verify_expected_host("", "bifrost").is_err());
    }

    #[test]
    fn ready_rest_plan_requires_writable_controls() {
        let plan = RestChangePlan {
            status: RestPlanStatus::Ready,
            changes: Vec::new(),
            blockers: Vec::new(),
            warnings: Vec::new(),
        };

        assert!(evaluate_rest_plan(&plan, true).is_ok());
        assert!(evaluate_rest_plan(&plan, false).is_err());
    }

    #[test]
    fn no_change_plan_does_not_require_writable_controls() {
        let plan = RestChangePlan {
            status: RestPlanStatus::NoChanges,
            changes: Vec::new(),
            blockers: Vec::new(),
            warnings: Vec::new(),
        };

        assert!(evaluate_rest_plan(&plan, false).is_ok());
    }
}
