//! Stable domain vocabulary shared by the engine and adapters.

pub mod intent;
pub mod node;
pub mod observation;
pub mod snapshot;

pub mod power_mode {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum PowerMode {
        /// Normal operating state with no Nordfir-imposed power restriction.
        Active,
        /// Low-power residency while the operating system and control path remain available.
        Rest,
        /// Node is intentionally powered down.
        Off,
    }
}

pub mod service {
    use super::node::NodeId;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ServiceId(pub String);

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProtectedService {
        pub id: ServiceId,
        pub node: NodeId,
        pub shutdown_protected: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ServiceActivity {
        Idle,
        Active { reason: String },
        Unknown { reason: String },
    }
}

pub use intent::*;
pub use node::*;
pub use observation::*;
pub use power_mode::*;
pub use service::*;
pub use snapshot::*;
