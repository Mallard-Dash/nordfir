use crate::core::{NodeId, PowerMode, ServiceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionRisk {
    Recoverable,
    Disruptive,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    SetPowerMode { node: NodeId, mode: PowerMode },
    WakeNode { node: NodeId },
    ShutdownNode { node: NodeId },
    StartService { service: ServiceId },
    StopService { service: ServiceId },
    Wait,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub kind: ActionKind,
    pub risk: ActionRisk,
    pub reason: String,
}
