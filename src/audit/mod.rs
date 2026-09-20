use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEventKind {
    IntentReceived,
    GuardEvaluated,
    AuthorizationRequested,
    AuthorizationGranted,
    AuthorizationDenied,
    PreflightCompleted,
    ActionExecuted,
    ActionFailed,
    RecoveryStateRetired,
    PolicyChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub at: SystemTime,
    pub actor: String,
    pub kind: AuditEventKind,
    pub message: String,
}

pub trait AuditSink: Send + Sync {
    fn record(&self, event: AuditEvent) -> Result<(), String>;
}

/// Newline-delimited append-mode audit sink for local development deployments.
#[derive(Debug, Clone)]
pub struct FileAuditSink {
    path: PathBuf,
}

impl FileAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl AuditSink for FileAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "audit path has no parent directory".to_owned())?;
        let parent_metadata = std::fs::symlink_metadata(parent)
            .map_err(|error| format!("inspect audit directory {}: {error}", parent.display()))?;
        if !parent_metadata.file_type().is_dir() || parent_metadata.file_type().is_symlink() {
            return Err("audit directory must be a real directory".to_owned());
        }

        if let Ok(metadata) = std::fs::symlink_metadata(&self.path) {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err("audit path must be a regular file".to_owned());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;

                if metadata.mode() & 0o077 != 0 {
                    return Err("audit log permissions must be 0600 or stricter".to_owned());
                }
                if metadata.uid() != parent_metadata.uid() {
                    return Err("audit log and directory must have the same owner".to_owned());
                }
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            if parent_metadata.mode() & 0o077 != 0 {
                return Err("audit directory must not grant group/other access".to_owned());
            }
            #[cfg(target_os = "linux")]
            if parent_metadata.uid() != effective_uid()? {
                return Err("audit directory must be owned by the effective user".to_owned());
            }
        }

        let timestamp = event
            .at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "audit timestamp predates the Unix epoch".to_owned())?
            .as_secs();
        let line = format!(
            "at={timestamp}\tactor={}\tkind={}\tmessage={}\n",
            escape_field(&event.actor),
            event_kind(&event.kind),
            escape_field(&event.message)
        );

        let mut options = OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&self.path)
            .map_err(|error| format!("open audit log {}: {error}", self.path.display()))?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.sync_data())
            .map_err(|error| format!("append audit log {}: {error}", self.path.display()))
    }
}

#[cfg(target_os = "linux")]
fn effective_uid() -> Result<u32, String> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|error| format!("read effective user id: {error}"))?;
    let uid_line = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .ok_or_else(|| "effective user id is unavailable".to_owned())?;
    uid_line
        .split_whitespace()
        .nth(2)
        .ok_or_else(|| "effective user id is malformed".to_owned())?
        .parse::<u32>()
        .map_err(|error| format!("parse effective user id: {error}"))
}

fn event_kind(kind: &AuditEventKind) -> &'static str {
    match kind {
        AuditEventKind::IntentReceived => "intent_received",
        AuditEventKind::GuardEvaluated => "guard_evaluated",
        AuditEventKind::AuthorizationRequested => "authorization_requested",
        AuditEventKind::AuthorizationGranted => "authorization_granted",
        AuditEventKind::AuthorizationDenied => "authorization_denied",
        AuditEventKind::PreflightCompleted => "preflight_completed",
        AuditEventKind::ActionExecuted => "action_executed",
        AuditEventKind::ActionFailed => "action_failed",
        AuditEventKind::RecoveryStateRetired => "recovery_state_retired",
        AuditEventKind::PolicyChanged => "policy_changed",
    }
}

fn escape_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use super::*;

    #[test]
    fn appends_escaped_audit_events() {
        let directory = std::env::temp_dir().join(format!(
            "nordfir-audit-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = directory.join("audit.log");
        let sink = FileAuditSink::new(&path);

        sink.record(AuditEvent {
            at: UNIX_EPOCH + Duration::from_secs(42),
            actor: "local\tuser".to_owned(),
            kind: AuditEventKind::ActionExecuted,
            message: "REST\napplied".to_owned(),
        })
        .unwrap();

        let content = fs::read_to_string(path).unwrap();
        assert_eq!(
            content,
            "at=42\tactor=local\\tuser\tkind=action_executed\tmessage=REST\\napplied\n"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
