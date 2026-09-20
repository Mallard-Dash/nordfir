use super::{NodeId, ServiceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvailabilityIntent {
    /// Keep the target immediately usable.
    Available,
    /// Minimize energy use while preserving the required control path and constraints.
    Economize,
    /// The target is no longer required to remain available.
    Release,
    /// Reserve the target for maintenance and suppress automatic optimization.
    Maintenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentTarget {
    Node(NodeId),
    Service(ServiceId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    pub target: IntentTarget,
    pub availability: AvailabilityIntent,
    pub reason: String,
}
