use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq)]
pub enum ObservationValue<T> {
    Known(T),
    Unknown { reason: String },
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Observation<T> {
    pub value: ObservationValue<T>,
    pub observed_at: SystemTime,
    pub source: String,
}

impl<T> Observation<T> {
    pub fn known(value: T, source: impl Into<String>) -> Self {
        Self::known_at(value, source, SystemTime::now())
    }

    pub fn known_at(value: T, source: impl Into<String>, observed_at: SystemTime) -> Self {
        Self {
            value: ObservationValue::Known(value),
            observed_at,
            source: source.into(),
        }
    }

    pub fn unknown(reason: impl Into<String>, source: impl Into<String>) -> Self {
        Self::unknown_at(reason, source, SystemTime::now())
    }

    pub fn unknown_at(
        reason: impl Into<String>,
        source: impl Into<String>,
        observed_at: SystemTime,
    ) -> Self {
        Self {
            value: ObservationValue::Unknown {
                reason: reason.into(),
            },
            observed_at,
            source: source.into(),
        }
    }

    pub fn unavailable(reason: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            value: ObservationValue::Unavailable {
                reason: reason.into(),
            },
            observed_at: SystemTime::now(),
            source: source.into(),
        }
    }

    pub fn age(&self, now: SystemTime) -> Option<Duration> {
        now.duration_since(self.observed_at).ok()
    }

    pub fn is_fresh(&self, now: SystemTime, max_age: Duration) -> bool {
        self.age(now).map(|age| age <= max_age).unwrap_or(false)
    }
}
