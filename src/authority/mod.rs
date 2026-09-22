//! Capability-based authorization types and policies.

pub mod capability {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Capability {
        Observe,
        OptimizePower,
        ControlApprovedServices,
        SuspendNode,
        ShutdownNode,
        RebootNode,
        EditTiming,
        EditProtectionRules,
        OverrideGuard,
        ManageAuthority,
    }
}

pub mod challenge {
    use super::capability::Capability;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AuthenticationFactor {
        MasterSecret,
        RecoveryPhrase,
        Totp,
        HardwareKey,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AuthorizationRequest {
        pub actor: String,
        pub capability: Capability,
        pub reason: String,
        pub requested_factors: Vec<AuthenticationFactor>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AuthorizationDecision {
        Allow,
        Deny {
            reason: String,
        },
        Challenge {
            required_factors: Vec<AuthenticationFactor>,
        },
    }
}

pub mod policy;

pub use capability::*;
pub use challenge::*;
pub use policy::*;

pub trait AuthorityGate: Send + Sync {
    fn authorize(&self, request: &AuthorizationRequest) -> AuthorizationDecision;
}
