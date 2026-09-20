use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub initial_delay: Duration,
    pub maximum_delay: Duration,
    pub maximum_attempts: Option<u32>,
}
