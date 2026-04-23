//! Connection state machine.
//!
//! Transitions are pure methods so they can be unit-tested without any
//! runtime or socket. The state object is also the source of truth for the
//! `status` command output.

use crate::domain::event::PeerInfo;
use crate::domain::id::ConnectionId;

/// Lifecycle status for a single connection (server or stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus
{
    Connected,
    Closing,
    Closed,
}

/// Per-connection state snapshot stored in the registry.
#[derive(Debug, Clone)]
pub struct ConnectionState
{
    pub id:        ConnectionId,
    pub status:    ConnectionStatus,
    pub label:     Option<String>,
    pub peer_info: Option<PeerInfo>,
}

impl ConnectionState
{
    /// Create a new state directly in `Connected` (for accepted connections
    /// where the transport is already established on arrival).
    pub fn new_connected(id: ConnectionId, peer_info: Option<PeerInfo>) -> Self
    {
        Self
        {
            id,
            status: ConnectionStatus::Connected,
            label: None,
            peer_info,
        }
    }

    /// Mark the connection as fully closed.
    pub fn on_closed(&mut self)
    {
        self.status = ConnectionStatus::Closed;
    }
}