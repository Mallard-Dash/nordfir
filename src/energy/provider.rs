use super::PowerReading;
use crate::core::NodeSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerReadError {
    Unavailable(String),
    Failed(String),
}

/// A source capable of producing a power reading or estimate.
///
/// Providers are read-only. Estimate providers may use normalized snapshot
/// data, while external/system providers can ignore it.
pub trait PowerProvider: Send + Sync {
    fn name(&self) -> &str;
    fn read_power(&self, snapshot: &NodeSnapshot) -> Result<PowerReading, PowerReadError>;
}

/// Tries providers in explicit priority order and returns the first usable
/// reading. Recommended order: external meter -> whole-system sensor ->
/// calibrated estimate -> generic estimate.
pub struct PowerReader {
    providers: Vec<Box<dyn PowerProvider>>,
}

impl PowerReader {
    pub fn new(providers: Vec<Box<dyn PowerProvider>>) -> Self {
        Self { providers }
    }

    pub fn read_best(&self, snapshot: &NodeSnapshot) -> Result<PowerReading, Vec<PowerReadError>> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.read_power(snapshot) {
                Ok(reading) => return Ok(reading),
                Err(error) => errors.push(error),
            }
        }
        Err(errors)
    }
}
