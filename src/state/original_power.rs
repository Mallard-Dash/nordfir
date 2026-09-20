use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
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
}
