use super::{Guard, GuardOutcome, GuardSeverity, GuardVerdict};
use crate::{
    action::{Action, ActionRisk},
    core::NodeSnapshot,
};
use std::time::{Duration, SystemTime};

pub struct FreshnessGuard {
    pub max_age: Duration,
}

impl Guard for FreshnessGuard {
    fn name(&self) -> &'static str {
        "observation-freshness"
    }

    fn evaluate(&self, snapshot: &NodeSnapshot, action: &Action) -> GuardVerdict {
        if matches!(&action.risk, ActionRisk::Recoverable) {
            return verdict(
                self.name(),
                GuardOutcome::Allow,
                "Recoverable action does not require destructive-action freshness",
            );
        }

        let now = SystemTime::now();
        let checks = [
            (
                "power mode",
                snapshot.power_mode.is_fresh(now, self.max_age),
            ),
            (
                "reachability",
                snapshot.reachable.is_fresh(now, self.max_age),
            ),
            (
                "SSH sessions",
                snapshot.active_ssh_sessions.is_fresh(now, self.max_age),
            ),
        ];

        if let Some((name, _)) = checks.into_iter().find(|(_, fresh)| !*fresh) {
            return verdict(
                self.name(),
                GuardOutcome::Block,
                format!("{name} observation is stale"),
            );
        }

        verdict(
            self.name(),
            GuardOutcome::Allow,
            "Safety-critical observations are fresh",
        )
    }
}

fn verdict(name: &'static str, outcome: GuardOutcome, reason: impl Into<String>) -> GuardVerdict {
    GuardVerdict {
        guard: name,
        severity: GuardSeverity::Critical,
        outcome,
        reason: reason.into(),
    }
}
