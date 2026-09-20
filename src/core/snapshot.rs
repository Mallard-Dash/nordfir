use std::collections::{BTreeMap, BTreeSet};

use crate::energy::PowerReading;
use super::{NodeId, Observation, PowerMode, ServiceActivity, ServiceId};

#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub node: NodeId,
    pub power_mode: Observation<PowerMode>,
    pub reachable: Observation<bool>,
    pub active_ssh_sessions: Observation<u32>,
    /// Services that must be accounted for before disruptive or performance-
    /// reducing operations are considered safe.
    pub protected_services: BTreeSet<ServiceId>,
    pub services: BTreeMap<ServiceId, Observation<ServiceActivity>>,
    pub cpu_utilization_percent: Option<Observation<f32>>,
    pub memory_utilization_percent: Option<Observation<f32>>,
    pub load_average_1m: Option<Observation<f32>>,
    pub uptime_seconds: Option<Observation<u64>>,
    pub cpu_frequency_mhz: Option<Observation<f32>>,
    pub power: Option<PowerReading>,
}
