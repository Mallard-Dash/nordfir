use crate::core::{Intent, IntentTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityWindow {
    pub target: IntentTarget,
    pub expression: String,
    pub intent: Intent,
    pub enabled: bool,
}
