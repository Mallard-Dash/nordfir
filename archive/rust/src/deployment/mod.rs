//! Read-only checks for the Linux service security boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxProcessSecurity {
    pub real_uid: u32,
    pub effective_uid: u32,
    pub saved_set_uid: u32,
    pub filesystem_uid: u32,
    pub no_new_privileges: bool,
    pub effective_capabilities: u64,
    pub bounding_capabilities: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityCheck {
    pub name: &'static str,
    pub result: Result<String, String>,
}

impl LinuxProcessSecurity {
    pub fn read_current() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            let status = std::fs::read_to_string("/proc/self/status")
                .map_err(|error| format!("read process security status: {error}"))?;
            Self::parse(&status)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err("process security inspection requires Linux /proc".to_owned())
        }
    }

    pub fn parse(status: &str) -> Result<Self, String> {
        let uid = field(status, "Uid:")?;
        let mut uid = uid.split_whitespace();
        let real_uid = parse_decimal(uid.next(), "real user id")?;
        let effective_uid = parse_decimal(uid.next(), "effective user id")?;
        let saved_set_uid = parse_decimal(uid.next(), "saved-set user id")?;
        let filesystem_uid = parse_decimal(uid.next(), "filesystem user id")?;
        if uid.next().is_some() {
            return Err("process user id field has unexpected values".to_owned());
        }

        let no_new_privileges = match field(status, "NoNewPrivs:")? {
            "0" => false,
            "1" => true,
            value => return Err(format!("invalid NoNewPrivs value '{value}'")),
        };

        Ok(Self {
            real_uid,
            effective_uid,
            saved_set_uid,
            filesystem_uid,
            no_new_privileges,
            effective_capabilities: parse_hex(field(status, "CapEff:")?, "CapEff")?,
            bounding_capabilities: parse_hex(field(status, "CapBnd:")?, "CapBnd")?,
        })
    }

    pub fn checks(&self) -> Vec<SecurityCheck> {
        vec![
            SecurityCheck {
                name: "Non-root identity",
                result: if self.effective_uid == 0 {
                    Err("effective user is root".to_owned())
                } else if self.real_uid != self.effective_uid
                    || self.saved_set_uid != self.effective_uid
                    || self.filesystem_uid != self.effective_uid
                {
                    Err(format!(
                        "user ids differ (real={}, effective={}, saved={}, filesystem={})",
                        self.real_uid, self.effective_uid, self.saved_set_uid, self.filesystem_uid
                    ))
                } else {
                    Ok(format!("running as non-root uid {}", self.effective_uid))
                },
            },
            SecurityCheck {
                name: "Privilege escalation",
                result: if self.no_new_privileges {
                    Ok("NoNewPrivs is enabled".to_owned())
                } else {
                    Err("NoNewPrivs is disabled".to_owned())
                },
            },
            SecurityCheck {
                name: "Effective capabilities",
                result: zero_capabilities(self.effective_capabilities, "CapEff"),
            },
            SecurityCheck {
                name: "Capability bounding set",
                result: zero_capabilities(self.bounding_capabilities, "CapBnd"),
            },
        ]
    }

    pub fn is_confined(&self) -> bool {
        self.checks().iter().all(|check| check.result.is_ok())
    }
}

fn field<'a>(status: &'a str, name: &str) -> Result<&'a str, String> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(name))
        .map(str::trim)
        .ok_or_else(|| format!("process status is missing {name}"))
}

fn parse_decimal(value: Option<&str>, name: &str) -> Result<u32, String> {
    value
        .ok_or_else(|| format!("process status is missing {name}"))?
        .parse()
        .map_err(|error| format!("parse {name}: {error}"))
}

fn parse_hex(value: &str, name: &str) -> Result<u64, String> {
    u64::from_str_radix(value, 16).map_err(|error| format!("parse {name}: {error}"))
}

fn zero_capabilities(value: u64, name: &str) -> Result<String, String> {
    if value == 0 {
        Ok(format!("{name} is empty"))
    } else {
        Err(format!("{name} contains 0x{value:016x}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFINED: &str = "Name:\tnordfir\nUid:\t991\t991\t991\t991\nCapEff:\t0000000000000000\nCapBnd:\t0000000000000000\nNoNewPrivs:\t1\n";

    #[test]
    fn parses_a_confined_non_root_process() {
        let security = LinuxProcessSecurity::parse(CONFINED).unwrap();

        assert_eq!(security.effective_uid, 991);
        assert!(security.no_new_privileges);
        assert!(security.is_confined());
    }

    #[test]
    fn root_is_not_a_least_privilege_service_account() {
        let status = CONFINED.replace("991\t991\t991\t991", "0\t0\t0\t0");
        let security = LinuxProcessSecurity::parse(&status).unwrap();

        assert!(!security.is_confined());
        assert!(
            security.checks()[0]
                .result
                .as_ref()
                .unwrap_err()
                .contains("root")
        );
    }

    #[test]
    fn capabilities_and_privilege_escalation_are_reported_separately() {
        let status = CONFINED
            .replace("CapEff:\t0000000000000000", "CapEff:\t0000000000000400")
            .replace("NoNewPrivs:\t1", "NoNewPrivs:\t0");
        let security = LinuxProcessSecurity::parse(&status).unwrap();
        let checks = security.checks();

        assert_eq!(checks.len(), 4);
        assert!(checks[1].result.is_err());
        assert!(checks[2].result.is_err());
        assert!(checks[3].result.is_ok());
    }

    #[test]
    fn malformed_process_status_fails_closed() {
        assert!(LinuxProcessSecurity::parse("Uid:\t1000\t1000\n").is_err());
        assert!(LinuxProcessSecurity::parse(&CONFINED.replace("NoNewPrivs:\t1", "")).is_err());
        assert!(
            LinuxProcessSecurity::parse(
                &CONFINED.replace("CapBnd:\t0000000000000000", "CapBnd:\tnot-hex"),
            )
            .is_err()
        );
    }
}
