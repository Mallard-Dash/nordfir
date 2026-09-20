use std::time::SystemTime;

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
