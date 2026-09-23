# Nordfir

Nordfir is a safety-aware energy saver for small servers and home labs.

The project is being restarted in Python. The first Rust implementation
(v0.5.13) is preserved in [`archive/rust/`](archive/rust/README.md), together
with its design documents.

## Design rules carried over from v0.5

- Unknown is not safe: missing information blocks an action.
- Capture the original state before changing anything, and verify every change.
- Prefer leaving a machine online over making an unsafe power-saving decision.
- Never run arbitrary shell commands; only typed, known actions.
- Record what was done and why.
