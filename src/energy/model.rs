use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct EnergyModel {
    pub active_idle_watts: Option<f32>,
    pub rest_watts: Option<f32>,
    pub off_watts: Option<f32>,
    pub electricity_price_per_kwh: Option<f32>,
    pub estimated_cycle_cost: Option<f32>,
    pub minimum_off_time: Duration,
}
