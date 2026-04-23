//! Typed connection identifier.
//!
//! This enum is the ONLY way to reference a connection in business logic.
//! Strings are parsed into `ConnectionId` in the REPL layer and formatted
//! back via `Display`. Never round-trip through `String` inside the domain.

use std::fmt;
use std::path::PathBuf;

/// Identifier for every object the registry tracks: servers, clients,
/// accepted connections.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionId
{
    /// A TCP server listening on a local port.
    TcpServer
    {
        port: u16,
    },

    /// A Unix-domain server bound to a local path.
    IpcServer
    {
        path: PathBuf,
    },

    /// An outgoing TCP client bound on a local port.
    TcpClient
    {
        local_port: u16,
    },

    /// An outgoing IPC client bound on a local path.
    IpcClient
    {
        local_path: PathBuf,
    },

    /// A connection accepted by a TCP server.
    AcceptedTcp
    {
        parent_port: u16,
        remote_port: u16,
        /// None when no collision; `Some(n)` (n starting at 1) on collision.
        disambig:    Option<u32>,
    },

    /// A connection accepted by an IPC server.
    AcceptedIpc
    {
        parent_path: PathBuf,
        pid:         i32,
        disambig:    Option<u32>,
    },
}

impl ConnectionId
{
    /// True if this id represents a server (listener + group of children).
    pub fn is_server(&self) -> bool
    {
        matches!(self, Self::TcpServer { .. } | Self::IpcServer { .. })
    }
}

impl fmt::Display for ConnectionId
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        match self
        {
            Self::TcpServer { port } =>
            {
                write!(f, "{port}")
            }
            Self::IpcServer { path } =>
            {
                write!(f, "{}", path.display())
            }
            Self::TcpClient { local_port } =>
            {
                write!(f, "{local_port}")
            }
            Self::IpcClient { local_path } =>
            {
                write!(f, "{}", local_path.display())
            }
            Self::AcceptedTcp { parent_port, remote_port, disambig } =>
            {
                match disambig
                {
                    Some(n) =>
                    {
                        write!(f, "{parent_port}.{remote_port}#{n}")
                    }
                    None =>
                    {
                        write!(f, "{parent_port}.{remote_port}")
                    }
                }
            }
            Self::AcceptedIpc { parent_path, pid, disambig } =>
            {
                match disambig
                {
                    Some(n) =>
                    {
                        write!(f, "{}.{}#{}", parent_path.display(), pid, n)
                    }
                    None =>
                    {
                        write!(f, "{}.{}", parent_path.display(), pid)
                    }
                }
            }
        }
    }
}