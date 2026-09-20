use std::sync::Mutex;

use crate::action::Action;
use super::Driver;

/// Non-destructive execution driver used while the control path is being proven.
/// It records typed actions but never writes to sysfs, calls systemd, or powers a host.
#[derive(Default)]
pub struct DryRunDriver {
    actions: Mutex<Vec<Action>>,
}

impl DryRunDriver {
    pub fn recorded_actions(&self) -> Vec<Action> {
        self.actions.lock().map(|items| items.clone()).unwrap_or_default()
    }
}

impl Driver for DryRunDriver {
    fn name(&self) -> &'static str { "dry-run" }
    fn supports(&self, _action: &Action) -> bool { true }
    fn execute(&self, action: &Action) -> Result<(), String> {
        self.actions.lock().map_err(|_| "dry-run action log lock poisoned".to_owned())?.push(action.clone());
        Ok(())
    }
}
