//! State collection for Nordfir.
//!
//! State collectors observe hosts and build normalized snapshots. They are
//! deliberately read-only and must not change power or service state.

mod linux;

pub use linux::*;
