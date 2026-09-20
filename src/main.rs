use std::process::ExitCode;

use nordfir::{
    authority::{AuthorityPolicy, AuthorizationDecision, PolicyAuthorityGate},
    core::{AvailabilityIntent, Intent, IntentTarget, NodeId},
    drivers::{Driver, DryRunDriver},
    energy::{GenericEstimateProvider, LinearPowerModel, PowerReader},
    engine::Engine,
    guards::{RestActivityGuard, SshSessionGuard},
    state::{LinuxPowerProbe, LinuxStateCollector},
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

    if command == "power-capabilities-local" {
        print_power_capabilities(&LinuxPowerProbe::default().probe());
        return Ok(());
    }

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
            "unknown command: {other}. Use inspect-local, power-capabilities-local or economize-local"
        )),
    }
}

fn print_power_capabilities(capabilities: &nordfir::energy::PowerCapabilities) {
    println!("Node: local");
    println!(
        "CPU frequency control: {}",
        availability(capabilities.cpufreq_available)
    );
    println!(
        "Current governor: {}",
        capabilities
            .current_governor
            .as_deref()
            .unwrap_or("Unavailable")
    );
    println!(
        "Available governors: {}",
        if capabilities.available_governors.is_empty() {
            "Unavailable".to_owned()
        } else {
            capabilities.available_governors.join(", ")
        }
    );
    println!(
        "Hardware frequency range: {}",
        frequency_range(capabilities.hardware_min_mhz, capabilities.hardware_max_mhz)
    );
    println!(
        "Configured frequency range: {}",
        frequency_range(capabilities.scaling_min_mhz, capabilities.scaling_max_mhz)
    );
    println!(
        "RAPL energy counters: {}",
        availability(capabilities.rapl_available)
    );
    println!(
        "Governor control file writable: {}",
        if capabilities.control_writable {
            "Yes (process authorization not verified)"
        } else {
            "No"
        }
    );
    println!("No system settings were changed.");
}

fn availability(available: bool) -> &'static str {
    if available {
        "Available"
    } else {
        "Unavailable"
    }
}

fn frequency_range(minimum: Option<f32>, maximum: Option<f32>) -> String {
    match (minimum, maximum) {
        (Some(minimum), Some(maximum)) => format!("{minimum:.0}-{maximum:.0} MHz"),
        _ => "Unavailable".to_owned(),
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
