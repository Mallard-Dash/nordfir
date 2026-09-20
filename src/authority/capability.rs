#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    Observe,
    OptimizePower,
    ControlApprovedServices,
    SuspendNode,
    ShutdownNode,
    RebootNode,
    EditTiming,
    EditProtectionRules,
    OverrideGuard,
    ManageAuthority,
}
