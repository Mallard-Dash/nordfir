use super::{Guard, GuardOutcome, GuardSeverity, GuardVerdict};
use crate::{
    action::{Action, ActionKind},
    core::{NodeSnapshot, ObservationValue},
};

pub struct SshSessionGuard;

impl Guard for SshSessionGuard {
    fn name(&self) -> &'static str {
        "ssh-session"
    }

    fn evaluate(&self, snapshot: &NodeSnapshot, action: &Action) -> GuardVerdict {
        if !matches!(&action.kind, ActionKind::ShutdownNode { .. }) {
            return verdict(
                self.name(),
                GuardOutcome::Allow,
                "Action does not power down the node",
            );
        }

        match &snapshot.active_ssh_sessions.value {
            ObservationValue::Known(0) => {
                verdict(self.name(), GuardOutcome::Allow, "No active SSH sessions")
            }
            ObservationValue::Known(count) => verdict(
                self.name(),
                GuardOutcome::Block,
                format!("{count} active SSH session(s)"),
            ),
            ObservationValue::Unknown { reason } | ObservationValue::Unavailable { reason } => {
                verdict(
                    self.name(),
                    GuardOutcome::Block,
                    format!("SSH session state is not safely known: {reason}"),
                )
            }
        }
    }
}

fn verdict(name: &'static str, outcome: GuardOutcome, reason: impl Into<String>) -> GuardVerdict {
    GuardVerdict {
        guard: name,
        severity: GuardSeverity::Hard,
        outcome,
        reason: reason.into(),
    }
}
