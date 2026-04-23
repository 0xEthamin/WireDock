//! Logger trait.
//!
//! Intentionally tiny and transport-agnostic. No dependency on any logging
//! crate. Infra ships a stderr-backed implementation.

/// Minimal logger contract used by wiredock internals.
pub trait Logger: Send + Sync
{
    /// Notable internal event that is still normal operation.
    fn info(&self, msg: &str);

    /// Recoverable or noteworthy issue.
    fn warn(&self, msg: &str);

    /// Serious internal failure.
    fn error(&self, msg: &str);
}