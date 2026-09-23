/// Read-only description of the power controls exposed by a node.
///
/// Capability discovery is deliberately separate from power-state changes. A
/// missing kernel interface is represented by an absent value instead of being
/// treated as a fatal error.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerCapabilities {
    pub cpufreq_available: bool,
    pub current_governor: Option<String>,
    pub available_governors: Vec<String>,
    pub hardware_min_mhz: Option<f32>,
    pub hardware_max_mhz: Option<f32>,
    pub scaling_min_mhz: Option<f32>,
    pub scaling_max_mhz: Option<f32>,
    pub rapl_available: bool,
    /// Whether the governor control file exposes write permission bits.
    ///
    /// This describes the sysfs interface, not whether the current process is
    /// authorized to change it.
    pub control_writable: bool,
}

impl PowerCapabilities {
    pub fn unavailable() -> Self {
        Self {
            cpufreq_available: false,
            current_governor: None,
            available_governors: Vec::new(),
            hardware_min_mhz: None,
            hardware_max_mhz: None,
            scaling_min_mhz: None,
            scaling_max_mhz: None,
            rapl_available: false,
            control_writable: false,
        }
    }
}
