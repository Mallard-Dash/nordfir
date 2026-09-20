use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::{core::NodeId, energy::PowerCapabilities};

const STATE_HEADER: &str = "NORDFIR_ORIGINAL_POWER_STATE_V1";
const STATE_VERSION: u8 = 1;

/// Original Linux power settings captured before a future REST transition.
#[derive(Debug, Clone, PartialEq)]
pub struct OriginalPowerState {
    pub version: u8,
    pub node: NodeId,
    pub captured_at_unix_seconds: u64,
    pub governor: String,
    pub scaling_min_mhz: f32,
    pub scaling_max_mhz: f32,
}

impl OriginalPowerState {
    pub fn capture(
        node: NodeId,
        capabilities: &PowerCapabilities,
        captured_at: SystemTime,
    ) -> Result<Self, String> {
        let governor = capabilities
            .current_governor
            .clone()
            .ok_or_else(|| "cannot snapshot an unknown CPU governor".to_owned())?;
        let scaling_min_mhz = capabilities
            .scaling_min_mhz
            .ok_or_else(|| "cannot snapshot an unknown minimum CPU frequency".to_owned())?;
        let scaling_max_mhz = capabilities
            .scaling_max_mhz
            .ok_or_else(|| "cannot snapshot an unknown maximum CPU frequency".to_owned())?;
        let captured_at_unix_seconds = captured_at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "snapshot timestamp predates the Unix epoch".to_owned())?
            .as_secs();

        let state = Self {
            version: STATE_VERSION,
            node,
            captured_at_unix_seconds,
            governor,
            scaling_min_mhz,
            scaling_max_mhz,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn encode(&self) -> Result<String, String> {
        self.validate()?;
        Ok(format!(
            "{STATE_HEADER}\nversion={}\nnode={}\ncaptured_at_unix_seconds={}\ngovernor={}\nscaling_min_mhz={}\nscaling_max_mhz={}\n",
            self.version,
            self.node,
            self.captured_at_unix_seconds,
            self.governor,
            self.scaling_min_mhz,
            self.scaling_max_mhz,
        ))
    }

    pub fn decode(content: &str) -> Result<Self, String> {
        let mut lines = content.lines();
        if lines.next() != Some(STATE_HEADER) {
            return Err("unsupported or missing original-power-state header".to_owned());
        }

        let mut fields = BTreeMap::new();
        for line in lines {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("invalid state line: {line}"))?;
            if fields.insert(key, value).is_some() {
                return Err(format!("duplicate state field: {key}"));
            }
        }
        if fields.len() != 6 {
            return Err("unexpected number of state fields".to_owned());
        }

        let state = Self {
            version: parse_field(&fields, "version")?,
            node: NodeId::new(required_field(&fields, "node")?),
            captured_at_unix_seconds: parse_field(&fields, "captured_at_unix_seconds")?,
            governor: required_field(&fields, "governor")?.to_owned(),
            scaling_min_mhz: parse_field(&fields, "scaling_min_mhz")?,
            scaling_max_mhz: parse_field(&fields, "scaling_max_mhz")?,
        };
        state.validate()?;
        Ok(state)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != STATE_VERSION {
            return Err(format!("unsupported state version: {}", self.version));
        }
        validate_token("node", &self.node.0)?;
        validate_token("governor", &self.governor)?;
        if !self.scaling_min_mhz.is_finite() || !self.scaling_max_mhz.is_finite() {
            return Err("CPU frequency values must be finite".to_owned());
        }
        if self.scaling_min_mhz <= 0.0 || self.scaling_max_mhz <= 0.0 {
            return Err("CPU frequency values must be positive".to_owned());
        }
        if self.scaling_min_mhz > self.scaling_max_mhz {
            return Err("minimum CPU frequency exceeds maximum CPU frequency".to_owned());
        }
        Ok(())
    }
}

/// File-backed state store that never overwrites an existing snapshot.
#[derive(Debug, Clone)]
pub struct OriginalPowerStateStore {
    root: PathBuf,
}

impl OriginalPowerStateStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn save(&self, state: &OriginalPowerState) -> Result<PathBuf, String> {
        let encoded = state.encode()?;
        self.prepare_root()?;
        let path = self.path_for(&state.node)?;

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut file = options
            .open(&path)
            .map_err(|error| format!("create {} without overwrite: {error}", path.display()))?;
        if let Err(error) = file
            .write_all(encoded.as_bytes())
            .and_then(|_| file.sync_all())
        {
            let _ = fs::remove_file(&path);
            return Err(format!("write {}: {error}", path.display()));
        }
        Ok(path)
    }

    pub fn load(&self, expected_node: &NodeId) -> Result<OriginalPowerState, String> {
        let path = self.path_for(expected_node)?;
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let state = OriginalPowerState::decode(&content)?;
        if &state.node != expected_node {
            return Err(format!(
                "state belongs to node '{}' instead of '{}'",
                state.node, expected_node
            ));
        }
        Ok(state)
    }

    /// Loads recovery state for a writable operation after checking local file security.
    pub fn load_for_write(&self, expected_node: &NodeId) -> Result<OriginalPowerState, String> {
        let path = self.path_for(expected_node)?;
        self.validate_secure_metadata(&path)?;
        self.load(expected_node)
    }

    pub fn load_fresh_for_write(
        &self,
        expected_node: &NodeId,
        now: SystemTime,
        maximum_age: std::time::Duration,
    ) -> Result<OriginalPowerState, String> {
        let state = self.load_for_write(expected_node)?;
        let now_seconds = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "current time predates the Unix epoch".to_owned())?
            .as_secs();
        if state.captured_at_unix_seconds > now_seconds {
            return Err("original-power-state timestamp is in the future".to_owned());
        }
        let age = now_seconds - state.captured_at_unix_seconds;
        if age > maximum_age.as_secs() {
            return Err(format!(
                "original-power-state is too old for apply: {age} seconds"
            ));
        }
        Ok(state)
    }

    #[cfg(unix)]
    fn validate_secure_metadata(&self, path: &Path) -> Result<(), String> {
        use std::os::unix::fs::MetadataExt;

        let directory = fs::symlink_metadata(&self.root)
            .map_err(|error| format!("inspect state directory {}: {error}", self.root.display()))?;
        if !directory.file_type().is_dir() || directory.file_type().is_symlink() {
            return Err("state directory must be a real directory".to_owned());
        }
        if directory.mode() & 0o077 != 0 {
            return Err("state directory permissions must not grant group/other access".to_owned());
        }
        #[cfg(target_os = "linux")]
        if directory.uid() != effective_uid()? {
            return Err("state directory must be owned by the effective user".to_owned());
        }

        let file = fs::symlink_metadata(path)
            .map_err(|error| format!("inspect state file {}: {error}", path.display()))?;
        if !file.file_type().is_file() || file.file_type().is_symlink() {
            return Err("original-power-state must be a regular file".to_owned());
        }
        if file.mode() & 0o077 != 0 {
            return Err("original-power-state permissions must be 0600 or stricter".to_owned());
        }
        if file.uid() != directory.uid() {
            return Err("state file and directory must have the same owner".to_owned());
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn validate_secure_metadata(&self, _path: &Path) -> Result<(), String> {
        Err("writable power operations require Unix file security checks".to_owned())
    }

    fn prepare_root(&self) -> Result<(), String> {
        let existed = self.root.exists();
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("create state directory {}: {error}", self.root.display()))?;
        if !self.root.is_dir() {
            return Err(format!(
                "state path is not a directory: {}",
                self.root.display()
            ));
        }
        #[cfg(unix)]
        if !existed {
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700)).map_err(
                |error| {
                    format!(
                        "set permissions on state directory {}: {error}",
                        self.root.display()
                    )
                },
            )?;
        }
        Ok(())
    }

    fn path_for(&self, node: &NodeId) -> Result<PathBuf, String> {
        validate_token("node", &node.0)?;
        Ok(self.root.join(format!("{}.original-power-state", node.0)))
    }
}

#[cfg(target_os = "linux")]
fn effective_uid() -> Result<u32, String> {
    let status = fs::read_to_string("/proc/self/status")
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

fn validate_token(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("invalid {label} value"));
    }
    Ok(())
}

fn required_field<'a>(fields: &'a BTreeMap<&str, &str>, name: &str) -> Result<&'a str, String> {
    fields
        .get(name)
        .copied()
        .ok_or_else(|| format!("missing state field: {name}"))
}

fn parse_field<T>(fields: &BTreeMap<&str, &str>, name: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    required_field(fields, name)?
        .parse::<T>()
        .map_err(|error| format!("invalid state field {name}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use super::*;

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn capabilities() -> PowerCapabilities {
        PowerCapabilities {
            cpufreq_available: true,
            current_governor: Some("performance".to_owned()),
            available_governors: vec!["performance".to_owned(), "powersave".to_owned()],
            hardware_min_mhz: Some(400.0),
            hardware_max_mhz: Some(4700.0),
            scaling_min_mhz: Some(400.0),
            scaling_max_mhz: Some(4700.0),
            rapl_available: false,
            control_writable: true,
        }
    }

    fn state() -> OriginalPowerState {
        OriginalPowerState::capture(
            NodeId::new("local"),
            &capabilities(),
            UNIX_EPOCH + Duration::from_secs(1234),
        )
        .expect("fixture state should be valid")
    }

    fn temporary_directory(test_name: &str) -> PathBuf {
        let suffix = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "nordfir-{test_name}-{}-{suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn state_format_round_trips() {
        let expected = state();
        let encoded = expected.encode().expect("state should encode");
        let decoded = OriginalPowerState::decode(&encoded).expect("state should decode");

        assert_eq!(decoded, expected);
    }

    #[test]
    fn capture_fails_when_original_values_are_unknown() {
        let result = OriginalPowerState::capture(
            NodeId::new("local"),
            &PowerCapabilities::unavailable(),
            UNIX_EPOCH,
        );

        assert!(result.is_err());
    }

    #[test]
    fn store_saves_loads_and_refuses_to_overwrite() {
        let directory = temporary_directory("state-store");
        let store = OriginalPowerStateStore::new(&directory);
        let expected = state();

        let path = store.save(&expected).expect("first save should succeed");
        let loaded = store
            .load(&NodeId::new("local"))
            .expect("saved state should load");
        let second_save = store.save(&expected);

        assert!(path.ends_with("local.original-power-state"));
        assert_eq!(loaded, expected);
        assert!(second_save.is_err());
        fs::remove_dir_all(directory).expect("temporary state directory should be removable");
    }

    #[test]
    fn load_rejects_state_for_another_node() {
        let directory = temporary_directory("wrong-node");
        fs::create_dir_all(&directory).expect("temporary directory should be created");
        fs::write(
            directory.join("other.original-power-state"),
            state().encode().expect("state should encode"),
        )
        .expect("state fixture should be written");

        let result = OriginalPowerStateStore::new(&directory).load(&NodeId::new("other"));

        assert!(
            result
                .expect_err("node mismatch should fail")
                .contains("state belongs to node")
        );
        fs::remove_dir_all(directory).expect("temporary state directory should be removable");
    }

    #[test]
    fn decode_rejects_unsupported_or_corrupt_state() {
        assert!(OriginalPowerState::decode("NORDFIR_ORIGINAL_POWER_STATE_V99\n").is_err());
        assert!(OriginalPowerState::decode("not-state\n").is_err());
        assert!(OriginalPowerState::decode(
            "NORDFIR_ORIGINAL_POWER_STATE_V1\nversion=1\nnode=local\ncaptured_at_unix_seconds=1234\ngovernor=performance\nscaling_min_mhz=400\nscaling_max_mhz=4700\nunexpected=value\n"
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn writable_load_rejects_insecure_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = temporary_directory("insecure-state");
        let store = OriginalPowerStateStore::new(&directory);
        let path = store.save(&state()).expect("state should save");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let result = store.load_for_write(&NodeId::new("local"));

        assert!(result.is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fresh_writable_load_rejects_old_and_future_snapshots() {
        let old_directory = temporary_directory("old-state");
        let old_store = OriginalPowerStateStore::new(&old_directory);
        old_store.save(&state()).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(10_000);

        assert!(
            old_store
                .load_fresh_for_write(&NodeId::new("local"), now, Duration::from_secs(900))
                .is_err()
        );

        let future_directory = temporary_directory("future-state");
        let future_store = OriginalPowerStateStore::new(&future_directory);
        let future = OriginalPowerState::capture(
            NodeId::new("local"),
            &capabilities(),
            UNIX_EPOCH + Duration::from_secs(20_000),
        )
        .unwrap();
        future_store.save(&future).unwrap();

        assert!(
            future_store
                .load_fresh_for_write(&NodeId::new("local"), now, Duration::from_secs(900))
                .is_err()
        );
        fs::remove_dir_all(old_directory).unwrap();
        fs::remove_dir_all(future_directory).unwrap();
    }

    #[test]
    fn fresh_writable_load_accepts_recent_snapshot() {
        let directory = temporary_directory("recent-state");
        let store = OriginalPowerStateStore::new(&directory);
        let recent = OriginalPowerState::capture(
            NodeId::new("local"),
            &capabilities(),
            UNIX_EPOCH + Duration::from_secs(9_500),
        )
        .unwrap();
        store.save(&recent).unwrap();

        let loaded = store
            .load_fresh_for_write(
                &NodeId::new("local"),
                UNIX_EPOCH + Duration::from_secs(10_000),
                Duration::from_secs(900),
            )
            .expect("recent secure state should load");

        assert_eq!(loaded, recent);
        fs::remove_dir_all(directory).unwrap();
    }
}
