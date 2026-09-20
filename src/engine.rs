use crate::{
    action::{Action, ActionKind, ActionRisk},
    authority::{AuthorityGate, AuthorizationDecision, AuthorizationRequest, Capability},
    core::{AvailabilityIntent, Intent, IntentTarget, NodeSnapshot, PowerMode},
    guards::{Guard, GuardOutcome, GuardVerdict},
};

pub struct Engine {
    guards: Vec<Box<dyn Guard>>,
    authority: Box<dyn AuthorityGate>,
}

#[derive(Debug)]
pub struct Evaluation {
    pub candidate: Option<Action>,
    pub guards: Vec<GuardVerdict>,
    pub authorization: Option<AuthorizationDecision>,
}

impl Engine {
    pub fn new(guards: Vec<Box<dyn Guard>>, authority: Box<dyn AuthorityGate>) -> Self {
        Self { guards, authority }
    }

    pub fn evaluate(&self, actor: &str, intent: &Intent, snapshot: &NodeSnapshot) -> Evaluation {
        let candidate = self.candidate_action(intent, snapshot);
        let Some(action) = candidate else {
            return Evaluation { candidate: None, guards: Vec::new(), authorization: None };
        };

        let verdicts: Vec<_> = self.guards.iter().map(|guard| guard.evaluate(snapshot, &action)).collect();
        if verdicts.iter().any(|v| matches!(v.outcome, GuardOutcome::Block | GuardOutcome::Defer)) {
            return Evaluation { candidate: Some(action), guards: verdicts, authorization: None };
        }

        let capability = capability_for(&action);
        let request = AuthorizationRequest {
            actor: actor.to_owned(), capability,
            reason: action.reason.clone(), requested_factors: Vec::new(),
        };
        let authorization = Some(self.authority.authorize(&request));

        Evaluation { candidate: Some(action), guards: verdicts, authorization }
    }

    fn candidate_action(&self, intent: &Intent, snapshot: &NodeSnapshot) -> Option<Action> {
        let IntentTarget::Node(node) = &intent.target else { return None; };
        match &intent.availability {
            AvailabilityIntent::Available => Some(Action {
                kind: ActionKind::SetPowerMode { node: node.clone(), mode: PowerMode::Active },
                risk: ActionRisk::Recoverable,
                reason: intent.reason.clone(),
            }),
            AvailabilityIntent::Economize => Some(Action {
                kind: ActionKind::SetPowerMode { node: node.clone(), mode: PowerMode::Rest },
                risk: ActionRisk::Recoverable,
                reason: intent.reason.clone(),
            }),
            AvailabilityIntent::Release => Some(Action {
                kind: ActionKind::ShutdownNode { node: node.clone() },
                risk: ActionRisk::Disruptive,
                reason: intent.reason.clone(),
            }),
            AvailabilityIntent::Maintenance => {
                let _ = snapshot;
                None
            }
        }
    }
}

fn capability_for(action: &Action) -> Capability {
    match &action.kind {
        ActionKind::SetPowerMode { .. } => Capability::OptimizePower,
        ActionKind::WakeNode { .. } => Capability::OptimizePower,
        ActionKind::ShutdownNode { .. } => Capability::ShutdownNode,
        ActionKind::StartService { .. } | ActionKind::StopService { .. } => Capability::ControlApprovedServices,
        ActionKind::Wait => Capability::Observe,
    }
}
