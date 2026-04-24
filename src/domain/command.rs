//! User commands after parsing.

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

    /// `open ipc client <remote_path>`
    ///
    /// No local path: the registry assigns a sequence id at open time.
    OpenIpcClient
    {
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