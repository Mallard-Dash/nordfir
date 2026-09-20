use std::time::SystemTime;

use crate::core::{NodeSnapshot, ObservationValue};
use super::{Confidence, LinearPowerModel, PowerProvider, PowerReadError, PowerReading, PowerScope, PowerSourceKind, UtilizationSample};

/// Deterministic whole-system estimate for hosts without external metering.
///
/// Disk and network utilization remain zero until dedicated observations are
/// added to NodeSnapshot. This limitation is reflected in the low confidence.
pub struct GenericEstimateProvider {
    model: LinearPowerModel,
}

impl GenericEstimateProvider {
    pub fn new(model: LinearPowerModel) -> Self { Self { model } }
}

impl PowerProvider for GenericEstimateProvider {
    fn name(&self) -> &str { "nordfir-generic-estimate" }

    fn read_power(&self, snapshot: &NodeSnapshot) -> Result<PowerReading, PowerReadError> {
        let cpu = percent(snapshot.cpu_utilization_percent.as_ref()).ok_or_else(|| PowerReadError::Unavailable("CPU utilization unavailable".to_owned()))?;
        let memory = percent(snapshot.memory_utilization_percent.as_ref()).ok_or_else(|| PowerReadError::Unavailable("memory utilization unavailable".to_owned()))?;
        let watts = self.model.estimate_watts(UtilizationSample { cpu, memory, disk: 0.0, network: 0.0 });
        Ok(PowerReading {
            watts,
            source: PowerSourceKind::GenericEstimate,
            scope: PowerScope::WholeSystem,
            confidence: Confidence::new(0.35),
            observed_at: SystemTime::now(),
            uncertainty_watts: self.model.typical_error_watts.or(Some(25.0)),
            provider: self.name().to_owned(),
        })
    }
}

fn percent(observation: Option<&crate::core::Observation<f32>>) -> Option<f32> {
    match &observation?.value {
        ObservationValue::Known(value) => Some((*value / 100.0).clamp(0.0, 1.0)),
        _ => None,
    }
}
