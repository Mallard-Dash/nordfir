use super::Capability;

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
