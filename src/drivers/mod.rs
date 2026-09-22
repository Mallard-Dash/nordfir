//! Typed action executors.
//!
//! A backend gets its own file once it has an implementation. Planned backends
//! remain inline so empty placeholder files do not obscure the active code.

pub mod dry_run;
pub mod linux_rest;

pub mod ipmi {
    //! Reserved for a typed IPMI backend.
}

pub mod local_power {
    //! Reserved for local suspend, shutdown and reboot actions.
}

pub mod ssh {
    //! Reserved for a typed remote execution backend without arbitrary shell input.
}

pub mod wol {
    //! Reserved for Wake-on-LAN actions.
}

use crate::action::Action;

pub trait Driver: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports(&self, action: &Action) -> bool;
    fn execute(&self, action: &Action) -> Result<(), String>;
}

pub use dry_run::DryRunDriver;
pub use linux_rest::{ActiveRestoreReport, LinuxRestDriver, RestApplyReport, RestoredSetting};
