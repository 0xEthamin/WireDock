//! Filesystem helpers for IPC socket paths.
//!
//! Philosophy: always unlink a stale socket file before `bind()` (recovery
//! from previous crashed run). Track every path wiredock created and unlink
//! each one again on shutdown. `Drop` on streams is NOT relied upon; the
//! registry calls these helpers explicitly.

use std::path::Path;

/// Remove a file if it exists. Ignores any error; this is a best-effort
/// pre-bind cleanup, not a security-sensitive operation.
pub fn unlink_if_stale(path: &Path)
{
    let _ = std::fs::remove_file(path);
}