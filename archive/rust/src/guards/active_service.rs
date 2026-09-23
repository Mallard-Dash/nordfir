use crate::{
    action::{Action, ActionKind},
    core::{NodeSnapshot, ObservationValue, ServiceActivity},
};

use super::{Guard, GuardOutcome, GuardSeverity, GuardVerdict};

/// Protects explicitly configured services from node shutdown.
///
/// An empty observation map is not evidence that an expected service is idle:
/// every service listed in `protected_services` must have a known idle state.
pub struct ActiveServiceGuard;

impl Guard for ActiveServiceGuard {
    fn name(&self) -> &'static str {
        "active-service"
    }

    fn evaluate(&self, snapshot: &NodeSnapshot, action: &Action) -> GuardVerdict {
        if !matches!(&action.kind, ActionKind::ShutdownNode { .. }) {
            return allow(self.name(), "Action does not power down the node");
        }

        for expected in &snapshot.protected_services {
            let Some(observation) = snapshot.services.get(expected) else {
                return GuardVerdict {
                    guard: self.name(),
                    severity: GuardSeverity::Hard,
                    outcome: GuardOutcome::Block,
                    reason: format!(
                        "Protected service {} has no activity observation",
                        expected.0
                    ),
                };
            };

            match &observation.value {
                ObservationValue::Known(ServiceActivity::Active { reason }) => {
                    return GuardVerdict {
                        guard: self.name(),
                        severity: GuardSeverity::Hard,
                        outcome: GuardOutcome::Block,
                        reason: format!("Protected service {} is active: {reason}", expected.0),
                    };
                }
                ObservationValue::Known(ServiceActivity::Unknown { reason })
                | ObservationValue::Unknown { reason }
                | ObservationValue::Unavailable { reason } => {
                    return GuardVerdict {
                        guard: self.name(),
                        severity: GuardSeverity::Hard,
                        outcome: GuardOutcome::Block,
                        reason: format!(
                            "Protected service {} activity is not safely known: {reason}",
                            expected.0
                        ),
                    };
                }
                ObservationValue::Known(ServiceActivity::Idle) => {}
            }
        }

        allow(
            self.name(),
            "All protected services are known idle or no protected services are configured",
        )
    }
}

fn allow(name: &'static str, reason: impl Into<String>) -> GuardVerdict {
    GuardVerdict {
        guard: name,
        severity: GuardSeverity::Hard,
        outcome: GuardOutcome::Allow,
        reason: reason.into(),
    }
}
