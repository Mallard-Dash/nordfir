pub mod capability;
pub mod challenge;
pub mod policy;

pub use capability::*;
pub use challenge::*;
pub use policy::*;

pub trait AuthorityGate: Send + Sync {
    fn authorize(&self, request: &AuthorizationRequest) -> AuthorizationDecision;
}
