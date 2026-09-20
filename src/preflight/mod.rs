use crate::{action::Action, core::{NodeId, NodeSnapshot}, guards::GuardVerdict};

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
