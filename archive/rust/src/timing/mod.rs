//! Scheduling and retry policy types.

pub mod retry {
    use std::time::Duration;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RetryPolicy {
        pub initial_delay: Duration,
        pub maximum_delay: Duration,
        pub maximum_attempts: Option<u32>,
    }
}

pub mod window {
    use crate::core::{Intent, IntentTarget};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AvailabilityWindow {
        pub target: IntentTarget,
        pub expression: String,
        pub intent: Intent,
        pub enabled: bool,
    }
}

pub use retry::*;
pub use window::*;
