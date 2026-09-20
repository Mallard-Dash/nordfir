//! Nordfir core engine.
//!
//! Nordfir is intentionally built without an integrated AI dependency.
//! Policy, safety and energy decisions must be deterministic, inspectable and testable.

pub mod action;
pub mod audit;
pub mod authority;
pub mod core;
pub mod drivers;
pub mod energy;
pub mod engine;
pub mod guards;
pub mod preflight;
pub mod timing;
pub mod watchers;

pub mod state;
