//! Stderr-backed logger.
//!
//! No external logging crate. This is the only concrete `Logger` in wiredock.

use std::io::Write;
use std::sync::Mutex;

use crate::domain::logger::Logger;

/// Logger that writes to stderr with a coarse level prefix.
///
/// Level strings are filtered by `min_level` (Trace=0, Info=1, Warn=2,
/// Error=3). The default is `Info`. Lines are serialized through a mutex
/// so they never interleave.
pub struct StderrLogger
{
    min_level: u8,
    lock:      Mutex<()>,
}

impl StderrLogger
{
    /// Create a logger with the given minimum level.
    pub fn new(min_level: u8) -> Self
    {
        Self { min_level, lock: Mutex::new(()) }
    }

    fn write(&self, level_num: u8, tag: &str, msg: &str)
    {
        if level_num < self.min_level
        {
            return;
        }
        // invariant: stderr is line-oriented; the lock guarantees no
        // interleaving between concurrent logger calls.
        let _guard = self.lock.lock().expect("stderr logger mutex poisoned");
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "[{tag}] {msg}");
    }
}

impl Logger for StderrLogger
{
    fn info (&self, msg: &str) { self.write(1, "INFO ", msg); }
    fn warn (&self, msg: &str) { self.write(2, "WARN ", msg); }
    fn error(&self, msg: &str) { self.write(3, "ERROR", msg); }
}