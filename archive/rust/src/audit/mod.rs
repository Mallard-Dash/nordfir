use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::{
    io::{BufRead, BufReader, Read},
    net::Shutdown,
    os::unix::{
        fs::OpenOptionsExt,
        net::{UnixDatagram, UnixStream},
    },
};

pub const AUDIT_FORWARD_SOCKET_ENV: &str = "NORDFIR_AUDIT_FORWARD_SOCKET";
pub const AUDIT_RECEIPT_SOCKET_ENV: &str = "NORDFIR_AUDIT_RECEIPT_SOCKET";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditLogStatus {
    ReadyToCreate,
    Ready,
}

/// Sends each audit event to every configured sink and reports all failures.
pub struct FanoutAuditSink {
    sinks: Vec<Box<dyn AuditSink>>,
}

impl FanoutAuditSink {
    pub fn new(sinks: Vec<Box<dyn AuditSink>>) -> Result<Self, String> {
        if sinks.is_empty() {
            return Err("audit fanout requires at least one sink".to_owned());
        }
        Ok(Self { sinks })
    }
}

impl AuditSink for FanoutAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        let mut failures = Vec::new();
        for (index, sink) in self.sinks.iter().enumerate() {
            if let Err(error) = sink.record(event.clone()) {
                failures.push(format!("sink {}: {error}", index + 1));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!("audit fanout failed: {}", failures.join("; ")))
        }
    }
}

/// Forwards newline-delimited audit events to a local Unix datagram collector.
#[derive(Debug, Clone)]
pub struct UnixDatagramAuditSink {
    path: PathBuf,
}

/// Sends an event to an external collector and requires an acceptance receipt.
#[derive(Debug, Clone)]
pub struct UnixReceiptAuditSink {
    path: PathBuf,
    timeout: Duration,
}

impl UnixDatagramAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Validates the forwarding destination without sending an event.
    pub fn inspect(&self) -> Result<(), String> {
        inspect_unix_socket(&self.path, "audit forwarding")
    }
}

impl AuditSink for UnixDatagramAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        self.inspect()?;
        #[cfg(unix)]
        {
            let line = encode_event(&event)?;
            let socket = UnixDatagram::unbound()
                .map_err(|error| format!("create audit forwarding socket: {error}"))?;
            let sent = socket
                .send_to(line.as_bytes(), &self.path)
                .map_err(|error| {
                    format!("forward audit event to {}: {error}", self.path.display())
                })?;
            if sent != line.len() {
                return Err(format!(
                    "forwarded {sent} of {} audit bytes to {}",
                    line.len(),
                    self.path.display()
                ));
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = event;
            Err("audit forwarding requires Unix datagram sockets".to_owned())
        }
    }
}

impl UnixReceiptAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Validates the receipt destination without connecting or sending an event.
    pub fn inspect(&self) -> Result<(), String> {
        inspect_unix_socket(&self.path, "audit receipt")
    }
}

impl AuditSink for UnixReceiptAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        self.inspect()?;
        #[cfg(unix)]
        {
            let line = encode_event(&event)?;
            let mut stream = UnixStream::connect(&self.path).map_err(|error| {
                format!(
                    "connect to audit receipt collector {}: {error}",
                    self.path.display()
                )
            })?;
            stream
                .set_read_timeout(Some(self.timeout))
                .map_err(|error| format!("configure audit receipt timeout: {error}"))?;
            stream
                .set_write_timeout(Some(self.timeout))
                .map_err(|error| format!("configure audit receipt timeout: {error}"))?;
            stream
                .write_all(line.as_bytes())
                .map_err(|error| format!("send audit event for receipt: {error}"))?;
            stream
                .shutdown(Shutdown::Write)
                .map_err(|error| format!("finish audit event for receipt: {error}"))?;

            let mut response = String::new();
            BufReader::new(stream.take(267))
                .read_line(&mut response)
                .map_err(|error| format!("read audit receipt: {error}"))?;
            parse_receipt(&response)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = event;
            Err("audit receipts require Unix stream sockets".to_owned())
        }
    }
}

impl FileAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Checks whether the audit log can be used without creating or changing it.
    pub fn inspect(&self) -> Result<AuditLogStatus, String> {
        let parent_metadata = self.validate_parent()?;
        match std::fs::symlink_metadata(&self.path) {
            Ok(metadata) => {
                self.validate_file(&metadata, &parent_metadata)?;
                Ok(AuditLogStatus::Ready)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;

                    if parent_metadata.mode() & 0o200 == 0 {
                        return Err("audit directory must be owner-writable".to_owned());
                    }
                }
                Ok(AuditLogStatus::ReadyToCreate)
            }
            Err(error) => Err(format!(
                "inspect audit path {}: {error}",
                self.path.display()
            )),
        }
    }

    fn validate_parent(&self) -> Result<std::fs::Metadata, String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "audit path has no parent directory".to_owned())?;
        let metadata = std::fs::symlink_metadata(parent)
            .map_err(|error| format!("inspect audit directory {}: {error}", parent.display()))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("audit directory must be a real directory".to_owned());
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            if metadata.mode() & 0o077 != 0 {
                return Err("audit directory must not grant group/other access".to_owned());
            }
            #[cfg(target_os = "linux")]
            if metadata.uid() != effective_uid()? {
                return Err("audit directory must be owned by the effective user".to_owned());
            }
        }
        Ok(metadata)
    }

    fn validate_file(
        &self,
        metadata: &std::fs::Metadata,
        parent_metadata: &std::fs::Metadata,
    ) -> Result<(), String> {
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("audit path must be a regular file".to_owned());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            if metadata.mode() & 0o077 != 0 {
                return Err("audit log permissions must be 0600 or stricter".to_owned());
            }
            if metadata.mode() & 0o200 == 0 {
                return Err("audit log must be owner-writable".to_owned());
            }
            if metadata.uid() != parent_metadata.uid() {
                return Err("audit log and directory must have the same owner".to_owned());
            }
        }
        Ok(())
    }
}

impl AuditSink for FileAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        let parent_metadata = self.validate_parent()?;

        match std::fs::symlink_metadata(&self.path) {
            Ok(metadata) => self.validate_file(&metadata, &parent_metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "inspect audit path {}: {error}",
                    self.path.display()
                ));
            }
        }

        let line = encode_event(&event)?;

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

fn encode_event(event: &AuditEvent) -> Result<String, String> {
    let timestamp = event
        .at
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "audit timestamp predates the Unix epoch".to_owned())?
        .as_secs();
    Ok(format!(
        "at={timestamp}\tactor={}\tkind={}\tmessage={}\n",
        escape_field(&event.actor),
        event_kind(&event.kind),
        escape_field(&event.message)
    ))
}

fn parse_receipt(response: &str) -> Result<&str, String> {
    let receipt = response
        .strip_suffix('\n')
        .and_then(|line| line.strip_prefix("accepted\t"))
        .ok_or_else(|| "audit collector returned an invalid receipt".to_owned())?;
    if receipt.is_empty() || receipt.len() > 256 {
        return Err("audit receipt id must contain 1 to 256 bytes".to_owned());
    }
    if !receipt
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err("audit receipt id contains invalid characters".to_owned());
    }
    Ok(receipt)
}

fn inspect_unix_socket(path: &std::path::Path, purpose: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{purpose} socket path must be absolute"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;

        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("inspect {purpose} socket {}: {error}", path.display()))?;
        if !metadata.file_type().is_socket() || metadata.file_type().is_symlink() {
            return Err(format!("{purpose} destination must be a Unix socket"));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Err(format!("{purpose} requires Unix sockets"))
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
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    #[cfg(unix)]
    use std::thread;

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

    #[test]
    fn inspect_reports_ready_before_and_after_creation() {
        let directory = std::env::temp_dir().join(format!(
            "nordfir-audit-status-{}-{}",
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
        let sink = FileAuditSink::new(directory.join("audit.log"));

        assert_eq!(sink.inspect().unwrap(), AuditLogStatus::ReadyToCreate);
        assert!(!directory.join("audit.log").exists());
        sink.record(AuditEvent {
            at: UNIX_EPOCH,
            actor: "local-user".to_owned(),
            kind: AuditEventKind::IntentReceived,
            message: "test".to_owned(),
        })
        .unwrap();
        assert_eq!(sink.inspect().unwrap(), AuditLogStatus::Ready);

        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inspect_rejects_a_non_writable_audit_log() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "nordfir-audit-readonly-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("audit.log");
        fs::write(&path, "existing event\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let result = FileAuditSink::new(&path).inspect();

        assert!(result.unwrap_err().contains("owner-writable"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn forwards_audit_events_to_a_unix_datagram_collector() {
        let directory = std::env::temp_dir().join(format!(
            "nordfir-audit-forward-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("collector.sock");
        let receiver = UnixDatagram::bind(&path).unwrap();
        let sink = UnixDatagramAuditSink::new(&path);

        sink.inspect().unwrap();
        sink.record(AuditEvent {
            at: UNIX_EPOCH + Duration::from_secs(84),
            actor: "local-user".to_owned(),
            kind: AuditEventKind::ActionExecuted,
            message: "REST applied".to_owned(),
        })
        .unwrap();

        let mut buffer = [0_u8; 256];
        let received = receiver.recv(&mut buffer).unwrap();
        assert_eq!(
            std::str::from_utf8(&buffer[..received]).unwrap(),
            "at=84\tactor=local-user\tkind=action_executed\tmessage=REST applied\n"
        );

        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn requires_a_valid_external_audit_receipt() {
        use std::os::unix::net::UnixListener;

        let directory = std::env::temp_dir().join(format!(
            "nordfir-audit-receipt-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("collector.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let collector = thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut event = String::new();
            connection.read_to_string(&mut event).unwrap();
            assert!(event.contains("kind=action_executed"));
            connection.write_all(b"accepted\treceipt-84\n").unwrap();
        });

        let sink = UnixReceiptAuditSink::new(&path);
        sink.record(AuditEvent {
            at: UNIX_EPOCH + Duration::from_secs(84),
            actor: "local-user".to_owned(),
            kind: AuditEventKind::ActionExecuted,
            message: "REST applied".to_owned(),
        })
        .unwrap();
        collector.join().unwrap();

        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn rejects_malformed_audit_receipts() {
        assert_eq!(
            parse_receipt("accepted\treceipt-84\n").unwrap(),
            "receipt-84"
        );
        assert!(parse_receipt("accepted\t\n").is_err());
        assert!(parse_receipt("accepted\tbad receipt\n").is_err());
        assert!(parse_receipt("rejected\treceipt-84\n").is_err());
    }

    #[test]
    fn fanout_attempts_every_sink_and_reports_failures() {
        struct CountingSink {
            calls: Arc<AtomicUsize>,
            fail: bool,
        }

        impl AuditSink for CountingSink {
            fn record(&self, _event: AuditEvent) -> Result<(), String> {
                self.calls.fetch_add(1, Ordering::Relaxed);
                if self.fail {
                    Err("collector unavailable".to_owned())
                } else {
                    Ok(())
                }
            }
        }

        let failed_calls = Arc::new(AtomicUsize::new(0));
        let successful_calls = Arc::new(AtomicUsize::new(0));
        let sink = FanoutAuditSink::new(vec![
            Box::new(CountingSink {
                calls: Arc::clone(&failed_calls),
                fail: true,
            }),
            Box::new(CountingSink {
                calls: Arc::clone(&successful_calls),
                fail: false,
            }),
        ])
        .unwrap();

        let result = sink.record(AuditEvent {
            at: UNIX_EPOCH,
            actor: "local-user".to_owned(),
            kind: AuditEventKind::IntentReceived,
            message: "test".to_owned(),
        });

        assert!(result.unwrap_err().contains("sink 1"));
        assert_eq!(failed_calls.load(Ordering::Relaxed), 1);
        assert_eq!(successful_calls.load(Ordering::Relaxed), 1);
    }
}
