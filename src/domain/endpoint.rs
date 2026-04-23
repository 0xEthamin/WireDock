//! Endpoint descriptors passed to the transport traits.
//!
//! Enums make inconsistent combinations (e.g. a TCP path, or an IPC host/port)
//! unrepresentable. The transport layer branches once on the enum and calls
//! the matching tokio API.

use std::path::PathBuf;

/// Client-side endpoint description.
#[derive(Debug, Clone)]
pub enum Endpoint
{
    /// TCP client: bind locally on `local_port`, connect to `remote_host:remote_port`.
    Tcp
    {
        local_port:  u16,
        remote_host: String,
        remote_port: u16,
    },

    /// IPC client: create `local_path` (removing stale file), connect to `remote_path`.
    ///
    /// An IPC client does not strictly need a bound local path to connect, but
    /// wiredock binds one anyway so the connection has a user-visible stable
    /// identifier consistent with TCP. See REPL and registry modules.
    Ipc
    {
        local_path:  PathBuf,
        remote_path: PathBuf,
    },
}

/// Server-side endpoint description.
#[derive(Debug, Clone)]
pub enum ServerEndpoint
{
    /// TCP listener on the given local port.
    Tcp
    {
        port: u16,
    },

    /// Unix-domain listener at the given path.
    Ipc
    {
        path: PathBuf,
    },
}