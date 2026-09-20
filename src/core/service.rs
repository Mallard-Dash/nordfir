use super::NodeId;

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
