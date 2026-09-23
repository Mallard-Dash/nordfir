//! Read-only fact collectors.
//!
//! Planned integrations stay inline until they contain executable behavior.

pub mod ping {
    //! Reserved for reachability observations.
}

pub mod power {
    //! Reserved for periodic power observations.
}

pub mod service {
    //! Reserved for protected-service activity observations.
}

pub mod ssh {
    //! Reserved for SSH-session observations.
}

use crate::core::{NodeId, NodeSnapshot};

pub trait Watcher: Send + Sync {
    fn name(&self) -> &'static str;
    fn refresh(&self, node: &NodeId, current: &mut NodeSnapshot) -> Result<(), String>;
}
