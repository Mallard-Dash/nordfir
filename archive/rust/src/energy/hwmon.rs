use std::{fs, path::PathBuf, time::SystemTime};

use super::{Confidence, PowerProvider, PowerReadError, PowerReading, PowerScope, PowerSourceKind};
use crate::core::NodeSnapshot;

/// Reads a configured Linux hwmon `power*_input` file.
///
/// Linux exposes these values in microwatts. Nordfir does not guess whether an
/// arbitrary hwmon sensor represents the whole host; the installer/configuration
/// must declare its scope.
pub struct HwmonPowerProvider {
    path: PathBuf,
    provider_name: String,
    source: PowerSourceKind,
    scope: PowerScope,
    confidence: Confidence,
}

impl HwmonPowerProvider {
    pub fn new(
        path: impl Into<PathBuf>,
        provider_name: impl Into<String>,
        source: PowerSourceKind,
        scope: PowerScope,
        confidence: Confidence,
    ) -> Self {
        Self {
            path: path.into(),
            provider_name: provider_name.into(),
            source,
            scope,
            confidence,
        }
    }
}

impl PowerProvider for HwmonPowerProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn read_power(&self, _snapshot: &NodeSnapshot) -> Result<PowerReading, PowerReadError> {
        let raw = fs::read_to_string(&self.path)
            .map_err(|e| PowerReadError::Unavailable(format!("{}: {e}", self.path.display())))?;
        let microwatts = raw
            .trim()
            .parse::<f32>()
            .map_err(|e| PowerReadError::Failed(format!("parse {}: {e}", self.path.display())))?;
        Ok(PowerReading {
            watts: microwatts / 1_000_000.0,
            source: self.source,
            scope: self.scope,
            confidence: self.confidence,
            observed_at: SystemTime::now(),
            uncertainty_watts: None,
            provider: self.provider_name.clone(),
        })
    }
}
