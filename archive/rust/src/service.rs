//! Read-only process loop for long-running observation.

use std::{thread, time::Duration};

use crate::core::{NodeSnapshot, Observation, ObservationValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObserverConfig {
    pub interval: Duration,
    pub maximum_cycles: Option<u64>,
}

impl ObserverConfig {
    pub fn new(interval: Duration, maximum_cycles: Option<u64>) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("observer interval must be greater than zero".to_owned());
        }
        if maximum_cycles == Some(0) {
            return Err("observer maximum cycles must be greater than zero".to_owned());
        }
        Ok(Self {
            interval,
            maximum_cycles,
        })
    }

    pub fn from_arguments(arguments: &[String]) -> Result<Self, String> {
        let (interval, maximum_cycles) = match arguments {
            [interval_flag, interval] if interval_flag == "--interval-seconds" => (interval, None),
            [interval_flag, interval, cycles_flag, cycles]
                if interval_flag == "--interval-seconds" && cycles_flag == "--max-cycles" =>
            {
                (interval, Some(cycles))
            }
            _ => {
                return Err(
                    "requires --interval-seconds <seconds> [--max-cycles <count>]".to_owned(),
                );
            }
        };
        let interval = interval
            .parse::<u64>()
            .map_err(|error| format!("parse observer interval: {error}"))?;
        let maximum_cycles = maximum_cycles
            .map(|cycles| {
                cycles
                    .parse::<u64>()
                    .map_err(|error| format!("parse observer maximum cycles: {error}"))
            })
            .transpose()?;
        Self::new(Duration::from_secs(interval), maximum_cycles)
    }
}

#[derive(Debug, Clone)]
pub struct ObserverHeartbeat {
    pub sequence: u64,
    pub snapshot: NodeSnapshot,
}

impl ObserverHeartbeat {
    pub fn encode_line(&self) -> String {
        let snapshot = &self.snapshot;
        let power = snapshot
            .power
            .as_ref()
            .map(|reading| format!("{:.1}", reading.watts))
            .unwrap_or_else(|| "missing".to_owned());
        format!(
            "observer heartbeat sequence={} node={} cpu_percent={} memory_percent={} load_1m={} uptime_seconds={} power_watts={}",
            self.sequence,
            snapshot.node,
            observation_value(snapshot.cpu_utilization_percent.as_ref()),
            observation_value(snapshot.memory_utilization_percent.as_ref()),
            observation_value(snapshot.load_average_1m.as_ref()),
            observation_value(snapshot.uptime_seconds.as_ref()),
            power
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObserverRunSummary {
    pub completed_cycles: u64,
}

pub trait ObserverRuntime {
    fn stop_requested(&self) -> bool;
    fn wait(&mut self, duration: Duration);
}

#[derive(Debug, Default)]
pub struct ThreadObserverRuntime;

impl ObserverRuntime for ThreadObserverRuntime {
    fn stop_requested(&self) -> bool {
        false
    }

    fn wait(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ObserverService {
    config: ObserverConfig,
}

impl ObserverService {
    pub fn new(config: ObserverConfig) -> Self {
        Self { config }
    }

    pub fn run<R, C, P>(
        &self,
        runtime: &mut R,
        mut collect: C,
        mut publish: P,
    ) -> Result<ObserverRunSummary, String>
    where
        R: ObserverRuntime,
        C: FnMut() -> Result<NodeSnapshot, String>,
        P: FnMut(&ObserverHeartbeat) -> Result<(), String>,
    {
        let mut completed_cycles = 0_u64;
        while !runtime.stop_requested() {
            let snapshot = collect().map_err(|error| {
                format!("observer collection failed after {completed_cycles} cycle(s): {error}")
            })?;
            completed_cycles += 1;
            publish(&ObserverHeartbeat {
                sequence: completed_cycles,
                snapshot,
            })
            .map_err(|error| {
                format!("observer publish failed during cycle {completed_cycles}: {error}")
            })?;

            if self.config.maximum_cycles == Some(completed_cycles) || runtime.stop_requested() {
                break;
            }
            runtime.wait(self.config.interval);
        }
        Ok(ObserverRunSummary { completed_cycles })
    }
}

fn observation_value<T: std::fmt::Display>(observation: Option<&Observation<T>>) -> String {
    match observation.map(|value| &value.value) {
        Some(ObservationValue::Known(value)) => value.to_string(),
        Some(ObservationValue::Unknown { .. }) => "unknown".to_owned(),
        Some(ObservationValue::Unavailable { .. }) => "unavailable".to_owned(),
        None => "missing".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        time::SystemTime,
    };

    use crate::core::{NodeId, Observation, PowerMode};

    use super::*;

    #[derive(Default)]
    struct RecordingRuntime {
        stop: bool,
        waits: Vec<Duration>,
    }

    impl ObserverRuntime for RecordingRuntime {
        fn stop_requested(&self) -> bool {
            self.stop
        }

        fn wait(&mut self, duration: Duration) {
            self.waits.push(duration);
        }
    }

    #[test]
    fn observer_configuration_rejects_non_progressing_limits() {
        assert!(ObserverConfig::new(Duration::ZERO, None).is_err());
        assert!(ObserverConfig::new(Duration::from_secs(1), Some(0)).is_err());
    }

    #[test]
    fn observer_configuration_parses_bounded_cli_arguments() {
        let arguments = vec![
            "--interval-seconds".to_owned(),
            "30".to_owned(),
            "--max-cycles".to_owned(),
            "3".to_owned(),
        ];

        let config = ObserverConfig::from_arguments(&arguments).unwrap();

        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.maximum_cycles, Some(3));
    }

    #[test]
    fn observer_runs_bounded_cycles_without_waiting_after_the_last() {
        let interval = Duration::from_secs(30);
        let service = ObserverService::new(ObserverConfig::new(interval, Some(3)).unwrap());
        let mut runtime = RecordingRuntime::default();
        let mut published = Vec::new();

        let summary = service
            .run(
                &mut runtime,
                || Ok(snapshot()),
                |heartbeat| {
                    published.push(heartbeat.sequence);
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(summary.completed_cycles, 3);
        assert_eq!(published, vec![1, 2, 3]);
        assert_eq!(runtime.waits, vec![interval, interval]);
    }

    #[test]
    fn heartbeat_has_a_compact_machine_readable_line() {
        let line = ObserverHeartbeat {
            sequence: 7,
            snapshot: snapshot(),
        }
        .encode_line();

        assert!(line.contains("sequence=7"));
        assert!(line.contains("node=test"));
        assert!(line.contains("cpu_percent=missing"));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn observer_honors_a_stop_request_before_collecting() {
        let service =
            ObserverService::new(ObserverConfig::new(Duration::from_secs(30), None).unwrap());
        let mut runtime = RecordingRuntime {
            stop: true,
            waits: Vec::new(),
        };
        let mut collections = 0;

        let summary = service
            .run(
                &mut runtime,
                || {
                    collections += 1;
                    Ok(snapshot())
                },
                |_| Ok(()),
            )
            .unwrap();

        assert_eq!(summary.completed_cycles, 0);
        assert_eq!(collections, 0);
        assert!(runtime.waits.is_empty());
    }

    fn snapshot() -> NodeSnapshot {
        let now = SystemTime::now();
        NodeSnapshot {
            node: NodeId::new("test"),
            power_mode: Observation::known_at(PowerMode::Active, "test", now),
            reachable: Observation::known_at(true, "test", now),
            active_ssh_sessions: Observation::known_at(0, "test", now),
            protected_services: BTreeSet::new(),
            services: BTreeMap::new(),
            cpu_utilization_percent: None,
            memory_utilization_percent: None,
            load_average_1m: None,
            uptime_seconds: None,
            cpu_frequency_mhz: None,
            power: None,
        }
    }
}
