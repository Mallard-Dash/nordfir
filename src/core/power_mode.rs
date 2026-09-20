#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PowerMode {
    /// Normal operating state with no Nordfir-imposed power restriction.
    Active,
    /// Low-power residency while the operating system and control path remain available.
    Rest,
    /// Node is intentionally powered down.
    Off,
}
