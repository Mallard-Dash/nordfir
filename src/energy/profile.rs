use crate::core::PowerMode;

#[derive(Debug, Clone, PartialEq)]
pub struct PowerProfile {
    pub mode: PowerMode,
    pub cpu_max_percent: Option<u8>,
    pub preserve_network: bool,
    pub preserve_ssh: bool,
}

impl PowerProfile {
    pub fn rest_default() -> Self {
        Self {
            mode: PowerMode::Rest,
            cpu_max_percent: Some(40),
            preserve_network: true,
            preserve_ssh: true,
        }
    }
}
