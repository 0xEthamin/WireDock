//! Endpoint descriptors passed to the transport traits.

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

    /// IPC client: connect to `remote_path`. No local bind.
    Ipc
    {
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