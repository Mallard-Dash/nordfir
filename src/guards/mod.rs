pub mod active_service;
pub mod freshness;
pub mod rest_activity;
pub mod ssh_session;

use crate::{action::Action, core::NodeSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuardSeverity {
    Soft,
    Hard,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardOutcome {
    Allow,
    Defer,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardVerdict {
    pub guard: &'static str,
    pub severity: GuardSeverity,
    pub outcome: GuardOutcome,
    pub reason: String,
}

pub trait Guard: Send + Sync {
    fn name(&self) -> &'static str;
    fn evaluate(&self, snapshot: &NodeSnapshot, action: &Action) -> GuardVerdict;
}

pub use rest_activity::RestActivityGuard;
pub use ssh_session::SshSessionGuard;
