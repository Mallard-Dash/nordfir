//! State collection for Nordfir.
//!
//! State collectors observe hosts and build normalized snapshots. They are
//! deliberately read-only and must not change power or service state.

mod linux;
mod linux_power;
mod original_power;

pub use linux::*;
pub use linux_power::*;
pub use original_power::*;
