//! User commands after parsing.
//!
//! The REPL layer produces values of this enum from raw input. The registry
//! consumes them and never re-parses strings except for the payload-target
//! id, which is resolved against the registry's id/label tables.

use std::path::PathBuf;

/// Parsed, validated user command.
#[derive(Debug, Clone)]
pub enum Command
{
    /// `open server <port>`
    OpenTcpServer
    {
        port: u16,
    },

    /// `open ipc server <path>`
    OpenIpcServer
    {
        path: PathBuf,
    },

    /// `open client <local_port> <host>:<port>`
    OpenTcpClient
    {
        local_port:  u16,
        remote_host: String,
        remote_port: u16,
    },

    /// `open ipc client <local_path> <remote_path>`
    OpenIpcClient
    {
        local_path:  PathBuf,
        remote_path: PathBuf,
    },

    /// `close <id>`
    Close
    {
        id_token: String,
    },

    /// `<id> <hex bytes...>`
    Send
    {
        id_token: String,
        bytes:    Vec<u8>,
    },

    /// `label <id> <name>`
    Label
    {
        id_token: String,
        name:     String,
    },

    /// `status`
    Status,
}