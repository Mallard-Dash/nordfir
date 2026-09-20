use crate::{
    action::{Action, ActionKind},
    core::{NodeSnapshot, ObservationValue, PowerMode, ServiceActivity},
};
use super::{Guard, GuardOutcome, GuardSeverity, GuardVerdict};

/// Conservative default protection for entering REST.
///
/// REST may reduce performance, so active protected services defer the action.
/// Missing or unknown activity for an expected protected service blocks it.
pub struct RestActivityGuard;

impl Guard for RestActivityGuard {
    fn name(&self) -> &'static str { "rest-activity" }

    fn evaluate(&self, snapshot: &NodeSnapshot, action: &Action) -> GuardVerdict {
        if !matches!(&action.kind, ActionKind::SetPowerMode { mode: PowerMode::Rest, .. }) {
            return verdict(self.name(), GuardOutcome::Allow, "Action does not enter REST mode");
        }

        for expected in &snapshot.protected_services {
            let Some(observation) = snapshot.services.get(expected) else {
                return verdict(self.name(), GuardOutcome::Block, format!("Protected service {} has no activity observation", expected.0));
            };
            match &observation.value {
                ObservationValue::Known(ServiceActivity::Idle) => {}
                ObservationValue::Known(ServiceActivity::Active { reason }) => {
                    return verdict(self.name(), GuardOutcome::Defer, format!("Protected service {} is active: {reason}", expected.0));
                }
                ObservationValue::Known(ServiceActivity::Unknown { reason })
                | ObservationValue::Unknown { reason }
                | ObservationValue::Unavailable { reason } => {
                    return verdict(self.name(), GuardOutcome::Block, format!("Protected service {} activity is unknown: {reason}", expected.0));
                }
            }
        }

        verdict(self.name(), GuardOutcome::Allow, "All protected services are idle or no protected services are configured")
    }
}

fn verdict(name: &'static str, outcome: GuardOutcome, reason: impl Into<String>) -> GuardVerdict {
    GuardVerdict { guard: name, severity: GuardSeverity::Hard, outcome, reason: reason.into() }
}
