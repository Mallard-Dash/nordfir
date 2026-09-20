use std::collections::BTreeSet;

use super::{
    AuthenticationFactor, AuthorityGate, AuthorizationDecision, AuthorizationRequest, Capability,
};

#[derive(Debug, Clone)]
pub struct AuthorityPolicy {
    pub granted: BTreeSet<Capability>,
    pub require_step_up_for: BTreeSet<Capability>,
}

impl AuthorityPolicy {
    pub fn observe_only() -> Self {
        Self {
            granted: BTreeSet::from([Capability::Observe]),
            require_step_up_for: BTreeSet::new(),
        }
    }

    /// Safe early-development policy: observation and REST-style optimization
    /// are allowed, but shutdown/reboot and authority changes are not.
    pub fn optimization_only() -> Self {
        Self {
            granted: BTreeSet::from([Capability::Observe, Capability::OptimizePower]),
            require_step_up_for: BTreeSet::new(),
        }
    }
}

pub struct PolicyAuthorityGate {
    policy: AuthorityPolicy,
}

impl PolicyAuthorityGate {
    pub fn new(policy: AuthorityPolicy) -> Self {
        Self { policy }
    }
}

impl AuthorityGate for PolicyAuthorityGate {
    fn authorize(&self, request: &AuthorizationRequest) -> AuthorizationDecision {
        if !self.policy.granted.contains(&request.capability) {
            return AuthorizationDecision::Deny {
                reason: format!("capability {:?} is not granted", request.capability),
            };
        }
        if self
            .policy
            .require_step_up_for
            .contains(&request.capability)
        {
            return AuthorizationDecision::Challenge {
                required_factors: vec![AuthenticationFactor::MasterSecret],
            };
        }
        AuthorizationDecision::Allow
    }
}
