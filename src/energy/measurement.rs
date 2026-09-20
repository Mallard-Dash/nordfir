use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PowerSourceKind {
    ExternalMeter,
    SystemSensor,
    ComponentSensor,
    CalibratedEstimate,
    GenericEstimate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerScope {
    WholeSystem,
    Component,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Confidence(pub f32);

impl Confidence {
    pub fn new(value: f32) -> Self { Self(value.clamp(0.0, 1.0)) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerReading {
    pub watts: f32,
    pub source: PowerSourceKind,
    pub scope: PowerScope,
    pub confidence: Confidence,
    pub observed_at: SystemTime,
    pub uncertainty_watts: Option<f32>,
    pub provider: String,
}

impl PowerReading {
    pub fn age(&self, now: SystemTime) -> Option<Duration> { now.duration_since(self.observed_at).ok() }
    pub fn is_fresh(&self, now: SystemTime, max_age: Duration) -> bool { self.age(now).is_some_and(|age| age <= max_age) }
}
