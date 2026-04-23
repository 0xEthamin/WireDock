//! Registry state and logic.
//!
//! Owned by a single tokio task. Writes to streams happen directly from this
//! task on the write half of each connection. This serializes writes globally,
//! which is acceptable for an interactive REPL (low throughput, bounded
//! concurrency) and keeps state transitions trivially auditable.
//!
//! Evolution path: per-connection writer tasks can be added later without
//! touching domain types. Nothing outside this module assumes writes are
//! synchronous or instantaneous.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::domain::error::DomainError;
use crate::domain::event::{CommandOutcome, DisplayLine, NetEvent, PeerInfo};
use crate::domain::hex::format_hex_spaced;
use crate::domain::id::ConnectionId;
use crate::domain::io::BoxedStream;
use crate::domain::state::{ConnectionState, ConnectionStatus};

/// A live accepted/client stream owned by the registry.
pub struct StreamHandle
{
    pub state:     ConnectionState,
    pub stream:    BoxedStream,
    pub read_task: JoinHandle<()>,
    /// For accepted connections: parent server id.
    pub parent:    Option<ConnectionId>,
    /// For IPC clients: local path to remove on close.
    pub local_ipc_path: Option<PathBuf>,
}

/// A live server owned by the registry.
pub struct ServerHandle
{
    pub state:         ConnectionState,
    pub accept_task:   JoinHandle<()>,
    pub children:      HashSet<ConnectionId>,
    /// IPC socket file path to remove on close (None for TCP).
    pub ipc_path: Option<PathBuf>,
    /// Per-remote-port collision counter (TCP) or per-pid (IPC).
    /// Key: the base identifier without disambig suffix.
    pub disambig_next: HashMap<String, u32>,
    /// Tracks base ids currently using no suffix, to trigger retro-renaming
    /// on first collision.
    pub base_no_suffix: HashMap<String, ConnectionId>,
}

/// Top-level registry state.
pub struct Registry
{
    pub streams:   HashMap<ConnectionId, StreamHandle>,
    pub servers:   HashMap<ConnectionId, ServerHandle>,
    /// Label -> id (1:1).
    pub labels:    HashMap<String, ConnectionId>,
    /// IPC paths created by this run, cleaned up on shutdown.
    pub ipc_paths: HashSet<PathBuf>,
}

impl Default for Registry
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl Registry
{
    pub fn new() -> Self
    {
        Self
        {
            streams:   HashMap::new(),
            servers:   HashMap::new(),
            labels:    HashMap::new(),
            ipc_paths: HashSet::new(),
        }
    }

    // ---------------------------------------------------------------------
    // Public API consumed by the runtime.
    //
    // The runtime owns the mpsc::Sender<RegistryInput> used by REPL and
    // connection tasks. The runtime also provides `open_*` closures that
    // can spawn real tasks; we inject behaviour via the `RegistryDeps`
    // struct so the domain stays runtime-clean but logic stays here.
    // ---------------------------------------------------------------------

    /// Render current `status` output per spec.
    pub fn render_status(&self) -> String
    {
        let mut out = String::new();
        out.push_str("CLIENTS\n");

        let mut client_ids: Vec<&ConnectionId> = self
            .streams
            .iter()
            .filter(|(_, h)| h.parent.is_none())
            .map(|(id, _)| id)
            .collect();
        client_ids.sort_by_key(|id| id.to_string());

        if client_ids.is_empty()
        {
            out.push_str("  (none)\n");
        }
        else
        {
            for id in client_ids
            {
                let h = &self.streams[id];
                let remote = match &h.state.peer_info
                {
                    Some(PeerInfo::TcpSocket { addr }) => addr.clone(),
                    Some(PeerInfo::IpcPid    { .. })  => "(ipc peer)".to_string(),
                    None                               => "?".to_string(),
                };
                let state = render_status_tag(h.state.status);
                let label = h
                    .state
                    .label
                    .as_ref()
                    .map(|n| format!("    label={n}"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {id} -> {remote}    [{state}]{label}\n"
                ));
            }
        }

        out.push_str("\nSERVERS\n");
        let mut server_ids: Vec<&ConnectionId> = self.servers.keys().collect();
        server_ids.sort_by_key(|id| id.to_string());

        if server_ids.is_empty()
        {
            out.push_str("  (none)\n");
        }
        else
        {
            for sid in server_ids
            {
                out.push_str(&format!("  {sid}\n"));
                let srv = &self.servers[sid];
                let mut children: Vec<&ConnectionId> = srv.children.iter().collect();
                children.sort_by_key(|id| id.to_string());
                for cid in children
                {
                    if let Some(h) = self.streams.get(cid)
                    {
                        let peer = h
                            .state
                            .peer_info
                            .as_ref()
                            .map(PeerInfo::render)
                            .unwrap_or_else(|| "?".to_string());
                        let state = render_status_tag(h.state.status);
                        let label = h
                            .state
                            .label
                            .as_ref()
                            .map(|n| format!("    label={n}"))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "    {cid}    peer={peer}    [{state}]{label}\n"
                        ));
                    }
                }
            }
        }

        out
    }

    // ---------------------------------------------------------------------
    // Command handling (pure state effects + write I/O).
    //
    // The runtime wraps this with the actual tokio plumbing. See repl::runtime.
    // ---------------------------------------------------------------------

    /// Resolve a user-typed token to a registered id, preferring labels.
    pub fn resolve_id(&self, token: &str, parser: impl Fn(&str) -> Option<ConnectionId>)
        -> Result<ConnectionId, DomainError>
    {
        if let Some(id) = self.labels.get(token)
        {
            return Ok(id.clone());
        }
        if let Some(id) = parser(token)
            && (self.streams.contains_key(&id) || self.servers.contains_key(&id))
            {
                return Ok(id);
            }
        Err(DomainError::UnknownId(token.to_string()))
    }

    /// Display name for an id (label if set, otherwise the id string).
    pub fn display_name(&self, id: &ConnectionId) -> String
    {
        if let Some(h) = self.streams.get(id)
            && let Some(l) = &h.state.label
            {
                return l.clone();
            }
        if let Some(s) = self.servers.get(id)
            && let Some(l) = &s.state.label
            {
                return l.clone();
            }
        id.to_string()
    }

    /// Set or replace a label.
    pub fn apply_label(&mut self, id: &ConnectionId, name: &str) -> Result<(), DomainError>
    {
        if let Some(existing) = self.labels.get(name)
            && existing != id
            {
                return Err(DomainError::LabelAlreadyInUse(name.to_string()));
            }

        // Remove old label if any.
        let old = self.current_label(id).cloned();
        if let Some(old_name) = old
        {
            self.labels.remove(&old_name);
        }

        if let Some(h) = self.streams.get_mut(id)
        {
            h.state.label = Some(name.to_string());
        }
        else if let Some(s) = self.servers.get_mut(id)
        {
            s.state.label = Some(name.to_string());
        }
        else
        {
            return Err(DomainError::UnknownId(id.to_string()));
        }
        self.labels.insert(name.to_string(), id.clone());
        Ok(())
    }

    fn current_label(&self, id: &ConnectionId) -> Option<&String>
    {
        if let Some(h) = self.streams.get(id)
        {
            return h.state.label.as_ref();
        }
        if let Some(s) = self.servers.get(id)
        {
            return s.state.label.as_ref();
        }
        None
    }

    /// Register a freshly opened stream (client or accepted) in the registry.
    pub fn insert_stream(&mut self, handle: StreamHandle)
    {
        let id = handle.state.id.clone();
        if let Some(parent_id) = &handle.parent
            && let Some(srv) = self.servers.get_mut(parent_id)
            {
                srv.children.insert(id.clone());
            }
        self.streams.insert(id, handle);
    }

    /// Remove a stream and abort its read task.
    pub fn drop_stream(&mut self, id: &ConnectionId)
    {
        if let Some(mut h) = self.streams.remove(id)
        {
            h.state.on_closed();
            h.read_task.abort();
            if let Some(parent) = &h.parent
                && let Some(srv) = self.servers.get_mut(parent)
                {
                    srv.children.remove(id);
                }
            // Drop on the stream closes the OS socket.
            drop(h.stream);
            if let Some(path) = h.local_ipc_path
            {
                let _ = std::fs::remove_file(&path);
                self.ipc_paths.remove(&path);
            }
            // Free label.
            let maybe_label = h.state.label.clone();
            if let Some(l) = maybe_label
            {
                self.labels.remove(&l);
            }
        }
    }

    /// Remove a server, cascading to every accepted child.
    ///
    /// Cascading semantics: closing a server aborts its accept loop AND
    /// closes every accepted connection attached to it, AND removes its
    /// IPC socket file if applicable. This is an intentional part of the
    /// user model: one command tears down the whole group.
    pub fn drop_server(&mut self, id: &ConnectionId)
    {
        if let Some(srv) = self.servers.remove(id)
        {
            srv.accept_task.abort();
            let children: Vec<ConnectionId> = srv.children.iter().cloned().collect();
            for c in children
            {
                self.drop_stream(&c);
            }
            if let Some(path) = srv.ipc_path
            {
                let _ = std::fs::remove_file(&path);
                self.ipc_paths.remove(&path);
            }
            if let Some(l) = srv.state.label
            {
                self.labels.remove(&l);
            }
        }
    }

    /// Send bytes. Handles both stream and server (broadcast) ids.
    ///
    /// Broadcast semantics (REQUIRED): writing to a server id sends to every
    /// accepted child. If one child write fails, the others still proceed.
    /// Each individual failure is reported as a separate `ERROR` event on
    /// the failing child's id via the display channel.
    pub async fn send_bytes(
        &mut self,
        target: &ConnectionId,
        bytes:  &[u8],
        display: &mpsc::Sender<DisplayLine>,
    ) -> Result<(), DomainError>
    {
        if bytes.is_empty()
        {
            return Err(DomainError::EmptyPayload);
        }

        if target.is_server()
        {
            let children: Vec<ConnectionId> = self
                .servers
                .get(target)
                .map(|s| s.children.iter().cloned().collect())
                .ok_or_else(|| DomainError::UnknownId(target.to_string()))?;

            if children.is_empty()
            {
                return Err(DomainError::ServerHasNoChildren(target.to_string()));
            }

            for c in children
            {
                if let Err(e) = self.write_to_stream(&c, bytes).await
                {
                    // Best-effort broadcast: report and continue.
                    let name = self.display_name(&c);
                    let _ = display
                        .send(DisplayLine::new(format!("[{name}] ERROR {e}")))
                        .await;
                }
            }
            return Ok(());
        }

        self.write_to_stream(target, bytes).await
    }

    async fn write_to_stream(
        &mut self,
        id: &ConnectionId,
        bytes: &[u8],
    ) -> Result<(), DomainError>
    {
        let h = self
            .streams
            .get_mut(id)
            .ok_or_else(|| DomainError::UnknownId(id.to_string()))?;
        if h.state.status == ConnectionStatus::Closed
            || h.state.status == ConnectionStatus::Closing
        {
            return Err(DomainError::Logic(format!("{id} is not writable")));
        }
        h.stream
            .write_all(bytes)
            .await
            .map_err(|e| DomainError::Io(e.to_string()))?;
        h.stream
            .flush()
            .await
            .map_err(|e| DomainError::Io(e.to_string()))?;
        Ok(())
    }

    /// Apply a received-bytes event: emit a RECV display line.
    pub async fn on_received(
        &self,
        id: &ConnectionId,
        bytes: &[u8],
        display: &mpsc::Sender<DisplayLine>,
    )
    {
        let name = self.display_name(id);
        let _ = display
            .send(DisplayLine::new(format!(
                "[{name}] RECV {}",
                format_hex_spaced(bytes)
            )))
            .await;
    }

    /// Apply a close event: emit a CLOSED line and remove the stream.
    pub async fn on_closed_event(
        &mut self,
        id: &ConnectionId,
        display: &mpsc::Sender<DisplayLine>,
    )
    {
        let name = self.display_name(id);
        self.drop_stream(id);
        let _ = display
            .send(DisplayLine::new(format!("[{name}] CLOSED")))
            .await;
    }

    /// Apply a per-connection error event.
    pub async fn on_error_event(
        &self,
        id: &ConnectionId,
        message: &str,
        display: &mpsc::Sender<DisplayLine>,
    )
    {
        let name = self.display_name(id);
        let _ = display
            .send(DisplayLine::new(format!("[{name}] ERROR {message}")))
            .await;
    }

    // ---------------------------------------------------------------------
    // Accepted-id disambiguation.
    // ---------------------------------------------------------------------

    /// Compute the final id for a newly accepted TCP connection, handling
    /// remote-port collisions (first-collision retro-rename included).
    ///
    /// Returns `(new_id, optional_rename_event)`.
    pub fn allocate_accepted_tcp_id(
        &mut self,
        parent_port: u16,
        remote_port: u16,
    ) -> (ConnectionId, Option<NetEvent>)
    {
        let base_key = format!("{parent_port}.{remote_port}");
        let parent = ConnectionId::TcpServer { port: parent_port };
        let srv = self
            .servers
            .get_mut(&parent)
            .expect("invariant: parent server exists when allocating accepted id");

        // Case 1: nothing uses this base yet.
        if !srv.base_no_suffix.contains_key(&base_key)
            && !srv.disambig_next.contains_key(&base_key)
        {
            let id = ConnectionId::AcceptedTcp
            {
                parent_port,
                remote_port,
                disambig: None,
            };
            srv.base_no_suffix.insert(base_key, id.clone());
            return (id, None);
        }

        // Case 2: we are about to collide with a no-suffix entry; retro-rename it.
        let rename_event;
        if let Some(old) = srv.base_no_suffix.remove(&base_key)
        {
            let new_first = ConnectionId::AcceptedTcp
            {
                parent_port,
                remote_port,
                disambig: Some(1),
            };
            // Update structures: streams map key, children set, label map.
            self.rename_accepted_tcp(&old, &new_first);
            rename_event = Some(NetEvent::Renamed
            {
                new_id: new_first,
                old_id: old,
            });
            // Re-borrow srv after self-mutation in rename_accepted_tcp.
            let srv2 = self
                .servers
                .get_mut(&parent)
                .expect("invariant: parent still present");
            srv2.disambig_next.insert(base_key.clone(), 2);
            let id = ConnectionId::AcceptedTcp
            {
                parent_port,
                remote_port,
                disambig: Some(2),
            };
            return (id, rename_event);
        }

        // Case 3: disambig already seeded; just take the next number.
        let n = srv.disambig_next.entry(base_key).or_insert(1);
        let this = *n;
        *n += 1;
        let id = ConnectionId::AcceptedTcp
        {
            parent_port,
            remote_port,
            disambig: Some(this),
        };
        (id, None)
    }

    /// Same logic for IPC accepted ids, keyed by pid.
    pub fn allocate_accepted_ipc_id(
        &mut self,
        parent_path: PathBuf,
        pid: i32,
    ) -> (ConnectionId, Option<NetEvent>)
    {
        let base_key = format!("{}.{}", parent_path.display(), pid);
        let parent   = ConnectionId::IpcServer { path: parent_path.clone() };
        let srv = self
            .servers
            .get_mut(&parent)
            .expect("invariant: parent ipc server exists");

        if !srv.base_no_suffix.contains_key(&base_key)
            && !srv.disambig_next.contains_key(&base_key)
        {
            let id = ConnectionId::AcceptedIpc
            {
                parent_path,
                pid,
                disambig: None,
            };
            srv.base_no_suffix.insert(base_key, id.clone());
            return (id, None);
        }

        let rename_event;
        if let Some(old) = srv.base_no_suffix.remove(&base_key)
        {
            let new_first = ConnectionId::AcceptedIpc
            {
                parent_path: parent_path.clone(),
                pid,
                disambig: Some(1),
            };
            self.rename_accepted_ipc(&old, &new_first);
            rename_event = Some(NetEvent::Renamed
            {
                new_id: new_first,
                old_id: old,
            });
            let srv2 = self.servers.get_mut(&parent).expect("invariant");
            srv2.disambig_next.insert(base_key.clone(), 2);
            let id = ConnectionId::AcceptedIpc
            {
                parent_path,
                pid,
                disambig: Some(2),
            };
            return (id, rename_event);
        }

        let n = srv.disambig_next.entry(base_key).or_insert(1);
        let this = *n;
        *n += 1;
        let id = ConnectionId::AcceptedIpc
        {
            parent_path,
            pid,
            disambig: Some(this),
        };
        (id, None)
    }

    fn rename_accepted_tcp(&mut self, old: &ConnectionId, new_id: &ConnectionId)
    {
        if let Some(mut h) = self.streams.remove(old)
        {
            h.state.id = new_id.clone();
            if let Some(parent) = &h.parent
                && let Some(srv) = self.servers.get_mut(parent)
                {
                    srv.children.remove(old);
                    srv.children.insert(new_id.clone());
                }
            self.streams.insert(new_id.clone(), h);
        }
    }

    fn rename_accepted_ipc(&mut self, old: &ConnectionId, new_id: &ConnectionId)
    {
        self.rename_accepted_tcp(old, new_id);
    }

    /// Shutdown: drop every server and every free client. Remove every
    /// tracked IPC socket file. Does not rely on `Drop` for cleanup.
    pub fn shutdown(&mut self)
    {
        let server_ids: Vec<ConnectionId> = self.servers.keys().cloned().collect();
        for sid in server_ids
        {
            self.drop_server(&sid);
        }
        let stream_ids: Vec<ConnectionId> = self.streams.keys().cloned().collect();
        for id in stream_ids
        {
            self.drop_stream(&id);
        }
        let paths: Vec<PathBuf> = self.ipc_paths.iter().cloned().collect();
        for p in paths
        {
            let _ = std::fs::remove_file(&p);
        }
        self.ipc_paths.clear();
    }

    // ---------------------------------------------------------------------
    // Label / state helpers for commands.
    // ---------------------------------------------------------------------

    /// Execute a parsed `Command::Label`.
    pub fn cmd_label(&mut self, id: &ConnectionId, name: &str) -> CommandOutcome
    {
        match self.apply_label(id, name)
        {
            Ok(())  => CommandOutcome::ok(format!("labeled {id} as {name}")),
            Err(e)  => CommandOutcome::err(e.to_string()),
        }
    }

    /// Execute a parsed `Command::Status`.
    pub fn cmd_status(&self) -> CommandOutcome
    {
        CommandOutcome::ok(self.render_status())
    }
}

fn render_status_tag(s: ConnectionStatus) -> &'static str
{
    match s
    {
        ConnectionStatus::Connected  => "CONNECTED",
        ConnectionStatus::Closing    => "CLOSING",
        ConnectionStatus::Closed     => "CLOSED",
    }
}

/// Helper passed to `resolve_id`: parse a token as `u16` (TCP port), else
/// treat it as an IPC path. Used by both REPL and send-path.
pub fn default_token_parser(token: &str) -> Option<ConnectionId>
{
    if let Ok(port) = token.parse::<u16>()
    {
        // Can match either TCP server or TCP client; caller reconciles with
        // the registry through `resolve_id`.
        return Some(ConnectionId::TcpClient { local_port: port });
    }
    if token.contains('.') && token.contains('#')
    {
        // Best-effort: try to parse `parent.remote#N` as AcceptedTcp.
        if let Some((left, right)) = token.split_once('#')
            && let Some((p, r)) = left.split_once('.')
                && let (Ok(pp), Ok(rp), Ok(n)) =
                    (p.parse::<u16>(), r.parse::<u16>(), right.parse::<u32>())
                {
                    return Some(ConnectionId::AcceptedTcp
                    {
                        parent_port: pp,
                        remote_port: rp,
                        disambig:    Some(n),
                    });
                }
    }
    if let Some((left, right)) = token.rsplit_once('.')
        && let (Ok(pp), Ok(rp)) = (left.parse::<u16>(), right.parse::<u16>())
        {
            return Some(ConnectionId::AcceptedTcp
            {
                parent_port: pp,
                remote_port: rp,
                disambig:    None,
            });
        }
    if token.starts_with('/')
    {
        return Some(ConnectionId::IpcClient { local_path: PathBuf::from(token) });
    }
    None
}