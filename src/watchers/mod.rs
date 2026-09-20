pub mod ping;
pub mod power;
pub mod service;
pub mod ssh;

use crate::core::{NodeId, NodeSnapshot};

pub trait Watcher: Send + Sync {
    fn name(&self) -> &'static str;
    fn refresh(&self, node: &NodeId, current: &mut NodeSnapshot) -> Result<(), String>;
}
