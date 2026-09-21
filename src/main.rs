use std::{
    path::Path,
    process::ExitCode,
    time::{Duration, SystemTime},
};

use nordfir::{
    audit::{
        AUDIT_FORWARD_SOCKET_ENV, AuditEvent, AuditEventKind, AuditLogStatus, AuditSink,
        FanoutAuditSink, FileAuditSink, UnixDatagramAuditSink,
    },
    authority::{AuthorityPolicy, AuthorizationDecision, PolicyAuthorityGate},
    core::{AvailabilityIntent, Intent, IntentTarget, NodeId},
    drivers::{Driver, DryRunDriver, LinuxRestDriver},
    energy::{
        GenericEstimateProvider, LinearPowerModel, PowerProfile, PowerReader, RestChange,
        RestChangePlan, RestPlanStatus, RestPlanner,
    },
    engine::Engine,
    guards::{RestActivityGuard, SshSessionGuard},
    preflight::{
        DeploymentPreflightReport, ReadinessStatus, evaluate_rest_plan, verify_expected_host,
    },
    state::{LinuxPowerProbe, LinuxStateCollector, OriginalPowerState, OriginalPowerStateStore},
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

    if command == "plan-rest-local" {
        let capabilities = LinuxPowerProbe::default().probe();
        let plan = RestPlanner::default().plan(&capabilities, &PowerProfile::rest_default());
        print_rest_plan(&plan, false);
        return match plan.status {
            RestPlanStatus::Ready | RestPlanStatus::NoChanges => Ok(()),
            RestPlanStatus::Blocked => Err("REST plan is blocked".to_owned()),
        };
    }

    if command == "save-original-state-local" {
        let state_directory = required_state_directory(&command)?;
        let state = OriginalPowerState::capture(
            NodeId::new("local"),
            &LinuxPowerProbe::default().probe(),
            SystemTime::now(),
        )?;
        let path = OriginalPowerStateStore::new(state_directory).save(&state)?;
        print_original_power_state(&state);
        println!("Saved state: {}", path.display());
        println!("No system power settings were changed.");
        return Ok(());
    }

    if command == "show-original-state-local" {
        let state_directory = required_state_directory(&command)?;
        let state = OriginalPowerStateStore::new(state_directory).load(&NodeId::new("local"))?;
        print_original_power_state(&state);
        println!("No system power settings were changed.");
        return Ok(());
    }

    if command == "lifecycle-status-local" {
        let state_directory = required_state_directory(&command)?;
        let node = NodeId::new("local");
        let status = OriginalPowerStateStore::new(&state_directory).inspect_lifecycle(&node)?;
        println!("Node: {node}");
        if !status.state_directory_initialized {
            println!("State directory: Not initialized");
            println!("Recovery phase: Idle");
            println!("Active recovery snapshot: Absent");
            println!("Archived recovery snapshots: 0");
            println!("Audit log: Not initialized");
        } else {
            println!("State directory: Ready");
            match status.active_snapshot {
                Some(state) => {
                    println!("Recovery phase: Armed");
                    println!(
                        "Active recovery snapshot: Ready (captured at Unix time {})",
                        state.captured_at_unix_seconds
                    );
                }
                None => {
                    println!("Recovery phase: Idle");
                    println!("Active recovery snapshot: Absent");
                }
            }
            println!("Archived recovery snapshots: {}", status.archived_snapshots);
            match file_audit_sink(&state_directory).inspect()? {
                AuditLogStatus::ReadyToCreate => {
                    println!("Audit log: Ready to create on first write");
                }
                AuditLogStatus::Ready => println!("Audit log: Ready"),
            }
        }
        match configured_forward_audit_sink()? {
            Some(sink) => {
                sink.inspect()?;
                println!("Audit forwarding: Ready ({})", sink.path().display());
            }
            None => println!("Audit forwarding: Disabled"),
        }
        println!("No system settings were changed.");
        return Ok(());
    }

    if command == "preflight-rest-local" {
        let state_directory = required_state_directory(&command)?;
        let expected_host = required_expected_host(&command)?;
        let observed_host = read_local_hostname();
        let mut report = DeploymentPreflightReport::default();

        report.record(
            "Host identity",
            observed_host.and_then(|observed| verify_expected_host(&expected_host, &observed)),
        );

        let node = NodeId::new("local");
        let store = OriginalPowerStateStore::new(&state_directory);
        report.record(
            "Recovery snapshot",
            store
                .load_fresh_for_write(&node, SystemTime::now(), Duration::from_secs(15 * 60))
                .map(|state| {
                    format!(
                        "secure and fresh (captured at Unix time {})",
                        state.captured_at_unix_seconds
                    )
                }),
        );

        let capabilities = LinuxPowerProbe::default().probe();
        let plan = RestPlanner::default().plan(&capabilities, &PowerProfile::rest_default());
        report.record(
            "REST plan",
            evaluate_rest_plan(&plan, capabilities.control_writable),
        );

        report.record(
            "Local audit",
            file_audit_sink(&state_directory)
                .inspect()
                .map(|status| match status {
                    AuditLogStatus::ReadyToCreate => "ready to create".to_owned(),
                    AuditLogStatus::Ready => "ready to append".to_owned(),
                }),
        );
        report.record(
            "Audit forwarding",
            match configured_forward_audit_sink() {
                Ok(Some(sink)) => sink
                    .inspect()
                    .map(|()| format!("ready ({})", sink.path().display())),
                Ok(None) => Ok("disabled; local audit remains enabled".to_owned()),
                Err(error) => Err(error),
            },
        );

        print_deployment_preflight(&report);
        print_rest_plan(&plan, false);
        return if report.is_ready() {
            Ok(())
        } else {
            Err("REST deployment preflight is blocked".to_owned())
        };
    }

    if command == "apply-rest-local" {
        let state_directory = required_state_directory(&command)?;
        require_write_confirmation(&command)?;
        let audit = audit_sink(&state_directory)?;
        record_audit(
            audit.as_ref(),
            AuditEventKind::IntentReceived,
            "REST apply requested",
        )?;
        let result =
            (|| {
                let node = NodeId::new("local");
                let original = OriginalPowerStateStore::new(&state_directory)
                    .load_fresh_for_write(&node, SystemTime::now(), Duration::from_secs(15 * 60))?;
                let capabilities = LinuxPowerProbe::default().probe();
                let plan =
                    RestPlanner::default().plan(&capabilities, &PowerProfile::rest_default());
                print_rest_plan(&plan, matches!(plan.status, RestPlanStatus::Ready));
                let report = LinuxRestDriver::default().apply(&plan, &original)?;
                println!("Applied changes: {}", report.applied.len());
                if report.applied.is_empty() {
                    println!("REST constraints were already satisfied; no settings were changed.");
                } else {
                    println!("REST settings were written and verified.");
                }
                Ok(())
            })();
        return finish_audited(audit.as_ref(), "REST apply", result);
    }

    if command == "restore-active-local" {
        let state_directory = required_state_directory(&command)?;
        require_write_confirmation(&command)?;
        let audit = audit_sink(&state_directory)?;
        record_audit(
            audit.as_ref(),
            AuditEventKind::IntentReceived,
            "ACTIVE restore requested",
        )?;
        let result = (|| {
            let node = NodeId::new("local");
            let store = OriginalPowerStateStore::new(&state_directory);
            let original = store.load_for_write(&node)?;
            let capabilities = LinuxPowerProbe::default().probe();
            print_original_power_state(&original);
            let report = LinuxRestDriver::default().restore_active(&original, &capabilities)?;
            println!("Restored settings: {}", report.restored.len());
            if report.restored.is_empty() {
                println!("Original ACTIVE settings were already present; nothing was changed.");
            } else {
                println!("Original ACTIVE settings were restored and verified.");
            }
            let archived = store.retire(&node)?;
            record_audit(
                audit.as_ref(),
                AuditEventKind::RecoveryStateRetired,
                &format!("recovery snapshot archived at {}", archived.display()),
            )?;
            println!("Archived recovery state: {}", archived.display());
            Ok(())
        })();
        return finish_audited(audit.as_ref(), "ACTIVE restore", result);
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
            "unknown command: {other}. Use inspect-local, power-capabilities-local, plan-rest-local, save-original-state-local, show-original-state-local, lifecycle-status-local, preflight-rest-local, apply-rest-local, restore-active-local or economize-local"
        )),
    }
}

fn file_audit_sink(state_directory: &str) -> FileAuditSink {
    FileAuditSink::new(Path::new(state_directory).join("audit.log"))
}

fn configured_forward_audit_sink() -> Result<Option<UnixDatagramAuditSink>, String> {
    match std::env::var_os(AUDIT_FORWARD_SOCKET_ENV) {
        Some(path) if path.is_empty() => {
            Err(format!("{AUDIT_FORWARD_SOCKET_ENV} must not be empty"))
        }
        Some(path) => Ok(Some(UnixDatagramAuditSink::new(path))),
        None => Ok(None),
    }
}

fn audit_sink(state_directory: &str) -> Result<Box<dyn AuditSink>, String> {
    let local: Box<dyn AuditSink> = Box::new(file_audit_sink(state_directory));
    match configured_forward_audit_sink()? {
        Some(forward) => Ok(Box::new(FanoutAuditSink::new(vec![
            local,
            Box::new(forward),
        ])?)),
        None => Ok(local),
    }
}

fn record_audit(audit: &dyn AuditSink, kind: AuditEventKind, message: &str) -> Result<(), String> {
    audit.record(AuditEvent {
        at: SystemTime::now(),
        actor: "local-user".to_owned(),
        kind,
        message: message.to_owned(),
    })
}

fn finish_audited(
    audit: &dyn AuditSink,
    operation: &str,
    result: Result<(), String>,
) -> Result<(), String> {
    match result {
        Ok(()) => {
            record_audit(
                audit,
                AuditEventKind::ActionExecuted,
                &format!("{operation} completed"),
            )?;
            Ok(())
        }
        Err(error) => {
            let audit_result = record_audit(
                audit,
                AuditEventKind::ActionFailed,
                &format!("{operation} failed: {error}"),
            );
            match audit_result {
                Ok(()) => Err(error),
                Err(audit_error) => Err(format!(
                    "{error}; additionally failed to record audit event: {audit_error}"
                )),
            }
        }
    }
}

fn require_write_confirmation(command: &str) -> Result<(), String> {
    const CONFIRMATION: &str = "--confirm-system-power-write";
    match std::env::args().nth(3).as_deref() {
        Some(CONFIRMATION) => Ok(()),
        _ => Err(format!(
            "{command} writes Linux power settings; rerun with <state-directory> {CONFIRMATION}"
        )),
    }
}

fn required_state_directory(command: &str) -> Result<String, String> {
    std::env::args()
        .nth(2)
        .ok_or_else(|| format!("{command} requires a <state-directory> argument"))
}

fn required_expected_host(command: &str) -> Result<String, String> {
    match (
        std::env::args().nth(3).as_deref(),
        std::env::args().nth(4),
    ) {
        (Some("--expect-host"), Some(host)) if !host.trim().is_empty() => Ok(host),
        _ => Err(format!(
            "{command} requires <state-directory> --expect-host <hostname>"
        )),
    }
}

fn read_local_hostname() -> Result<String, String> {
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map_err(|error| format!("read local hostname: {error}"))?;
    let hostname = hostname.trim();
    if hostname.is_empty() {
        Err("local hostname is empty".to_owned())
    } else {
        Ok(hostname.to_owned())
    }
}

fn print_deployment_preflight(report: &DeploymentPreflightReport) {
    for check in &report.checks {
        let status = match check.status {
            ReadinessStatus::Ready => "Ready",
            ReadinessStatus::Blocked => "Blocked",
        };
        println!("Preflight {}: {} - {}", check.name, status, check.detail);
    }
    println!(
        "Preflight status: {}",
        if report.is_ready() {
            "Ready"
        } else {
            "Blocked"
        }
    );
}

fn print_original_power_state(state: &OriginalPowerState) {
    println!("Node: {}", state.node);
    println!("State version: {}", state.version);
    println!("Captured at Unix time: {}", state.captured_at_unix_seconds);
    println!("Original governor: {}", state.governor);
    println!(
        "Original configured frequency range: {:.0}-{:.0} MHz",
        state.scaling_min_mhz, state.scaling_max_mhz
    );
}

fn print_rest_plan(plan: &RestChangePlan, apply: bool) {
    println!("Node: local");
    println!("Requested mode: Rest");
    println!("Plan status: {:?}", plan.status);
    for change in &plan.changes {
        match change {
            RestChange::CpuGovernor { current, target } => {
                println!("CPU governor: {current} -> {target}");
            }
            RestChange::CpuMaxFrequencyMhz { current, target } => {
                println!("CPU maximum: {current:.0} -> {target:.0} MHz");
            }
        }
    }
    for blocker in &plan.blockers {
        println!("Blocker: {blocker}");
    }
    for warning in &plan.warnings {
        println!("Warning: {warning}");
    }
    println!("Apply: {apply}");
    if !apply {
        println!("No system settings were changed.");
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
