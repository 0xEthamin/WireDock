//! Network events and multiplexed registry input.
//!
//! The registry consumes a single channel (`mpsc::Receiver<RegistryInput>`).
//! Commands come from the REPL, events come from connection read tasks and
//! accept tasks. This unifies scheduling and keeps state exclusively owned
//! by the registry.

use crate::domain::command::Command;
use crate::domain::id::ConnectionId;

/// Remote-peer descriptor, independent of the concrete transport.
#[derive(Debug, Clone)]
pub enum PeerInfo
{
    /// TCP remote: IP:port as a string (to avoid leaking `std::net` types here).
    TcpSocket
    {
        addr: String,
    },
    /// IPC peer: PID resolved via SO_PEERCRED.
    IpcPid
    {
        pid: i32,
    },
}

impl PeerInfo
{
    /// Rendering used in status and events (no `peer=` prefix).
    pub fn render(&self) -> String
    {
        match self
        {
            Self::TcpSocket { addr } => addr.clone(),
            Self::IpcPid    { pid }  => format!("pid={pid}"),
        }
    }
}

/// Asynchronous network event emitted by accept and read tasks.
#[derive(Debug)]
pub enum NetEvent
{
    /// Raw bytes received on a stream.
    Received
    {
        id:    ConnectionId,
        bytes: Vec<u8>,
    },

    /// A stream closed (EOF or remote close).
    Closed
    {
        id: ConnectionId,
    },

    /// Per-connection failure.
    Error
    {
        id:      ConnectionId,
        message: String,
    },

    /// An accepted-connection id was renamed due to a remote-port collision.
    Renamed
    {
        new_id: ConnectionId,
        old_id: ConnectionId,
    },
}

/// One line of display output, already formatted.
#[derive(Debug, Clone)]
pub struct DisplayLine
{
    pub text: String,
}

impl DisplayLine
{
    pub fn new<S: Into<String>>(text: S) -> Self
    {
        Self { text: text.into() }
    }
}

/// Merged input for the registry task.
///
/// `Accepted` is separated from `NetEvent` because the accept path also
/// hands off the concrete `BoxedStream`, which cannot appear in the pure
/// event enum if we want to keep `NetEvent: Debug` clean and testable in
/// isolation.
pub enum RegistryInput
{
    /// Command from the REPL.
    Command
    {
        cmd:   Command,
        reply: tokio::sync::oneshot::Sender<CommandOutcome>,
    },

    /// Asynchronous network event from a read task.
    Event(NetEvent),

    /// Accepted connection handoff from an accept task.
    Accepted
    {
        parent:    ConnectionId,
        child:     ConnectionId,
        peer_info: PeerInfo,
        stream:    crate::domain::io::BoxedStream,
    },

    /// Shutdown notification (from Ctrl+C handler in main loop).
    Shutdown,
}

impl std::fmt::Debug for RegistryInput 
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result 
    {
        match self 
        {
            Self::Command { cmd, .. } => write!(f, "Command({cmd:?})"),
            Self::Event(ev) => write!(f, "Event({ev:?})"),
            Self::Accepted { parent, child, peer_info, .. } => 
            {
                write!(f, "Accepted {{ parent: {parent}, child: {child}, peer_info: {peer_info:?} }}")
            }
            Self::Shutdown => write!(f, "Shutdown"),
        }
    }
}

/// Result of executing a user command.
#[derive(Debug, Clone)]
pub struct CommandOutcome
{
    pub success: bool,
    pub message: String,
}

impl CommandOutcome
{
    pub fn ok<S: Into<String>>(msg: S) -> Self
    {
        Self { success: true,  message: msg.into() }
    }

    pub fn err<S: Into<String>>(msg: S) -> Self
    {
        Self { success: false, message: msg.into() }
    }
}