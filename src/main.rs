use std::process::ExitCode;

use nordfir::{
    authority::{AuthorityPolicy, AuthorizationDecision, PolicyAuthorityGate},
    core::{AvailabilityIntent, Intent, IntentTarget, NodeId},
    drivers::{Driver, DryRunDriver},
    energy::{GenericEstimateProvider, LinearPowerModel, PowerReader},
    engine::Engine,
    guards::{RestActivityGuard, SshSessionGuard},
    state::LinuxStateCollector,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Nordfir error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "inspect-local".to_owned());
    let node = NodeId::new("local");
    let collector = LinuxStateCollector::default();
    let mut snapshot = collector.collect(node.clone())?;

    let power_reader = PowerReader::new(vec![Box::new(GenericEstimateProvider::new(
        LinearPowerModel {
            baseline_watts: 35.0,
            cpu_watts_per_unit: 65.0,
            memory_watts_per_unit: 12.0,
            disk_watts_per_unit: 0.0,
            network_watts_per_unit: 0.0,
            typical_error_watts: Some(20.0),
        },
    ))]);

    if let Ok(reading) = power_reader.read_best(&snapshot) {
        snapshot.power = Some(reading);
    }

    match command.as_str() {
        "inspect-local" => {
            print_snapshot(&snapshot);
            Ok(())
        }
        "economize-local" => {
            let authority = PolicyAuthorityGate::new(AuthorityPolicy::optimization_only());
            let engine = Engine::new(
                vec![Box::new(RestActivityGuard), Box::new(SshSessionGuard)],
                Box::new(authority),
            );
            let intent = Intent {
                target: IntentTarget::Node(node),
                availability: AvailabilityIntent::Economize,
                reason: "Local dry-run energy optimization".to_owned(),
            };
            let evaluation = engine.evaluate("local-user", &intent, &snapshot);
            println!("Intent: Economize");
            for verdict in &evaluation.guards {
                println!(
                    "Guard {}: {:?} - {}",
                    verdict.guard, verdict.outcome, verdict.reason
                );
            }
            match (&evaluation.candidate, &evaluation.authorization) {
                (Some(action), Some(AuthorizationDecision::Allow)) => {
                    let driver = DryRunDriver::default();
                    driver.execute(action)?;
                    println!("Dry-run action: {:?}", action.kind);
                    println!("No system settings were changed.");
                    Ok(())
                }
                (_, Some(AuthorizationDecision::Deny { reason })) => {
                    Err(format!("authorization denied: {reason}"))
                }
                (_, Some(AuthorizationDecision::Challenge { .. })) => {
                    Err("step-up authentication is required".to_owned())
                }
                _ => Err("no executable action was produced".to_owned()),
            }
        }
        other => Err(format!(
            "unknown command: {other}. Use inspect-local or economize-local"
        )),
    }
}

fn print_snapshot(snapshot: &nordfir::core::NodeSnapshot) {
    println!("Node: {}", snapshot.node);
    if let Some(cpu) = &snapshot.cpu_utilization_percent {
        println!("CPU: {:?}", cpu.value);
    }
    if let Some(memory) = &snapshot.memory_utilization_percent {
        println!("Memory: {:?}", memory.value);
    }
    if let Some(load) = &snapshot.load_average_1m {
        println!("Load 1m: {:?}", load.value);
    }
    if let Some(uptime) = &snapshot.uptime_seconds {
        println!("Uptime seconds: {:?}", uptime.value);
    }
    if let Some(power) = &snapshot.power {
        println!(
            "Power: {:.1} W ({:?}, confidence {:.0}%)",
            power.watts,
            power.source,
            power.confidence.0 * 100.0
        );
        if let Some(uncertainty) = power.uncertainty_watts {
            println!("Power uncertainty: ±{uncertainty:.1} W");
        }
    }
}
