use std::time::Duration;
use super::EnergyModel;

#[derive(Debug, Clone, PartialEq)]
pub struct EnergyEstimate {
    pub expected_idle: Duration,
    pub estimated_saving: Option<f32>,
    pub break_even: Option<Duration>,
    pub confidence: EstimateConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateConfidence { Low, Medium, High }

pub fn estimate_shutdown(model: &EnergyModel, expected_idle: Duration) -> EnergyEstimate {
    let saving = match (model.rest_watts, model.off_watts, model.electricity_price_per_kwh) {
        (Some(rest), Some(off), Some(price)) if rest > off => {
            let hours = expected_idle.as_secs_f32() / 3600.0;
            Some(((rest - off) / 1000.0) * hours * price)
        }
        _ => None,
    };

    let break_even = match (model.rest_watts, model.off_watts, model.electricity_price_per_kwh, model.estimated_cycle_cost) {
        (Some(rest), Some(off), Some(price), Some(cycle)) if rest > off && price > 0.0 => {
            let savings_per_hour = ((rest - off) / 1000.0) * price;
            let hours = cycle / savings_per_hour;
            Some(Duration::from_secs_f32(hours * 3600.0))
        }
        _ => None,
    };

    EnergyEstimate { expected_idle, estimated_saving: saving, break_even, confidence: EstimateConfidence::Low }
}
