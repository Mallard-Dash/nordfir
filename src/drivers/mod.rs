pub mod dry_run;
pub mod ipmi;
pub mod linux_rest;
pub mod local_power;
pub mod ssh;
pub mod wol;

use crate::action::Action;

pub trait Driver: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports(&self, action: &Action) -> bool;
    fn execute(&self, action: &Action) -> Result<(), String>;
}

pub use dry_run::DryRunDriver;
pub use linux_rest::{LinuxRestDriver, RestApplyReport};
