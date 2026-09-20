/// Minimal deterministic calibration model for estimating whole-system power.
///
/// This is intentionally not AI/ML infrastructure. The coefficients can be
/// derived later from ordinary regression against real wall-power samples.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearPowerModel {
    pub baseline_watts: f32,
    pub cpu_watts_per_unit: f32,
    pub memory_watts_per_unit: f32,
    pub disk_watts_per_unit: f32,
    pub network_watts_per_unit: f32,
    pub typical_error_watts: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UtilizationSample {
    /// Normalized 0.0..=1.0 utilization values.
    pub cpu: f32,
    pub memory: f32,
    pub disk: f32,
    pub network: f32,
}

impl LinearPowerModel {
    pub fn estimate_watts(&self, sample: UtilizationSample) -> f32 {
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        self.baseline_watts
            + self.cpu_watts_per_unit * clamp(sample.cpu)
            + self.memory_watts_per_unit * clamp(sample.memory)
            + self.disk_watts_per_unit * clamp(sample.disk)
            + self.network_watts_per_unit * clamp(sample.network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_model_is_deterministic() {
        let model = LinearPowerModel {
            baseline_watts: 80.0,
            cpu_watts_per_unit: 100.0,
            memory_watts_per_unit: 20.0,
            disk_watts_per_unit: 10.0,
            network_watts_per_unit: 5.0,
            typical_error_watts: Some(8.0),
        };
        let watts = model.estimate_watts(UtilizationSample {
            cpu: 0.5,
            memory: 0.5,
            disk: 0.0,
            network: 0.0,
        });
        assert_eq!(watts, 140.0);
    }
}
