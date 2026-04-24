//! Registry task + connection read/accept tasks.
//!
//! Tokio tasks here are thin: they forward I/O events to the registry via
//! `RegistryInput`. All business logic lives in `Registry`. Tasks are
//! cancelled with `JoinHandle::abort()` only; no cancellation tokens.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::domain::command::Command;
use crate::domain::endpoint::{Endpoint, ServerEndpoint};
use crate::domain::error::DomainError;
use crate::domain::event::{
    CommandOutcome, DisplayLine, NetEvent, PeerInfo, RegistryInput,
};
use crate::domain::id::ConnectionId;
use crate::domain::io::BoxedStream;
use crate::domain::logger::Logger;
use crate::domain::registry::{default_token_parser, Registry, ServerHandle, StreamHandle};
use crate::domain::state::ConnectionState;
use crate::domain::transport::{Connector, Listener};

/// Dependencies handed to the registry task.
pub struct RuntimeDeps<C, L>
where
    C: Connector + 'static,
    L: Listener  + 'static,
{
    pub connector: C,
    pub listener:  L,
    pub logger:    Arc<dyn Logger>,
    pub display:   mpsc::Sender<DisplayLine>,
    pub input_tx:  mpsc::Sender<RegistryInput>,
    pub input_rx:  mpsc::Receiver<RegistryInput>,
}

/// Entry point of the registry task.
///
/// Runs until a `Shutdown` input is received. On shutdown, the registry
/// walks its own state and closes every resource explicitly, then returns.
/// Callers are expected to `await` this task's `JoinHandle` after sending
/// `Shutdown` to observe clean termination.
pub async fn run_registry<C, L>(mut deps: RuntimeDeps<C, L>)
where
    C: Connector + Clone + 'static,
    L: Listener  + Clone + 'static,
{
    let mut reg = Registry::new();
    // Tracks handle of accept tasks we spawn so we can hand ownership back
    // into the registry when inserting ServerHandle entries.
    let _ = &mut reg;

    loop
    {
        let Some(msg) = deps.input_rx.recv().await else { break; };
        match msg
        {
            RegistryInput::Shutdown =>
            {
                deps.logger.info("shutdown requested");
                break;
            }
            RegistryInput::Command { cmd, reply } =>
            {
                let outcome = handle_command(
                    &mut reg,
                    cmd,
                    &deps.connector,
                    &deps.listener,
                    &deps.input_tx,
                    &deps.display,
                )
                .await;
                // invariant: reply receiver is held by the caller for this
                // command; if it has been dropped, we just log and continue.
                if reply.send(outcome).is_err()
                {
                    deps.logger.warn("command reply channel closed before response");
                }
            }
            RegistryInput::Event(ev) =>
            {
                handle_event(&mut reg, ev, &deps.display).await;
            }
            RegistryInput::Accepted { parent, child, peer_info, stream } =>
            {
                handle_accepted(
                    &mut reg,
                    parent,
                    child,
                    peer_info,
                    stream,
                    &deps.input_tx,
                    &deps.display,
                )
                .await;
            }
        }
    }

    // Clean shutdown: explicit teardown, no reliance on Drop inside tasks.
    reg.shutdown();
    deps.logger.info("registry shutdown complete");
}

async fn handle_command<C, L>(
    reg:       &mut Registry,
    cmd:       Command,
    connector: &C,
    listener:  &L,
    input_tx:  &mpsc::Sender<RegistryInput>,
    display:   &mpsc::Sender<DisplayLine>,
) -> CommandOutcome
where
    C: Connector + Clone + 'static,
    L: Listener  + Clone + 'static,
{
    match cmd
    {
        Command::OpenTcpServer { port } =>
        {
            let id = ConnectionId::TcpServer { port };
            if reg.servers.contains_key(&id) || reg.streams.contains_key(&id)
            {
                return CommandOutcome::err(
                    DomainError::IdAlreadyInUse(port.to_string()).to_string(),
                );
            }
            match listener.bind(ServerEndpoint::Tcp { port }).await
            {
                Ok(mut al) =>
                {
                    let accept_tx = input_tx.clone();
                    let parent_id = id.clone();
                    let task = tokio::spawn(async move
                    {
                        loop
                        {
                            match al.accept().await
                            {
                                Ok(accepted) =>
                                {
                                    // Derive child id based on peer.
                                    let child_probe = match &accepted.peer_info
                                    {
                                        PeerInfo::TcpSocket { addr } =>
                                        {
                                            parse_remote_port(addr)
                                        }
                                        PeerInfo::IpcPid { .. } | PeerInfo::IpcRemotePath { .. } =>
                                        {
                                            // Not expected on a TCP accept loop: IPC peer variants cannot
                                            // originate from a TcpListener. Ignore defensively.
                                            None
                                        }
                                    };
                                    if let Some(remote_port) = child_probe
                                    {
                                        let tentative = ConnectionId::AcceptedTcp
                                        {
                                            parent_port: port,
                                            remote_port,
                                            disambig:    None,
                                        };
                                        let _ = accept_tx.send(RegistryInput::Accepted
                                        {
                                            parent:    parent_id.clone(),
                                            child:     tentative,
                                            peer_info: accepted.peer_info,
                                            stream:    accepted.stream,
                                        }).await;
                                    }
                                }
                                Err(e) =>
                                {
                                    let _ = accept_tx.send(RegistryInput::Event(
                                        NetEvent::Error
                                        {
                                            id:      parent_id.clone(),
                                            message: format!("accept failed: {e}"),
                                        }
                                    )).await;
                                    // Keep the loop alive: transient errors should
                                    // not kill the server. If the listener is
                                    // permanently broken, errors will keep flowing
                                    // until `close` is issued.
                                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                }
                            }
                        }
                    });
                    let srv = ServerHandle
                    {
                        state:          ConnectionState::new_connected(id.clone(), None),
                        accept_task:    task,
                        children:       Default::default(),
                        ipc_path:       None,
                        disambig_next:  HashMap::new(),
                        base_no_suffix: HashMap::new(),
                    };
                    reg.servers.insert(id.clone(), srv);
                    let _ = display.send(DisplayLine::new(
                        format!("[{id}] listening")
                    )).await;
                    CommandOutcome::ok(format!("listening on {id}"))
                }
                Err(e) => CommandOutcome::err(e.to_string()),
            }
        }
        Command::OpenIpcServer { path } =>
        {
            let id = ConnectionId::IpcServer { path: path.clone() };
            if reg.servers.contains_key(&id)
            {
                return CommandOutcome::err(
                    DomainError::IdAlreadyInUse(path.display().to_string()).to_string(),
                );
            }
            match listener.bind(ServerEndpoint::Ipc { path: path.clone() }).await
            {
                Ok(mut al) =>
                {
                    let accept_tx = input_tx.clone();
                    let parent_id = id.clone();
                    let parent_path = path.clone();
                    let task = tokio::spawn(async move
                    {
                        loop
                        {
                            match al.accept().await
                            {
                                Ok(accepted) =>
                                {
                                    match &accepted.peer_info
                                    {
                                        PeerInfo::IpcPid { pid } =>
                                        {
                                            let tentative = ConnectionId::AcceptedIpc
                                            {
                                                parent_path: parent_path.clone(),
                                                pid:         *pid,
                                                disambig:    None,
                                            };
                                            let _ = accept_tx.send(RegistryInput::Accepted
                                            {
                                                parent:    parent_id.clone(),
                                                child:     tentative,
                                                peer_info: accepted.peer_info,
                                                stream:    accepted.stream,
                                            }).await;
                                        }
                                        PeerInfo::TcpSocket { .. } | PeerInfo::IpcRemotePath { .. } =>
                                        {
                                            // Not expected on an IPC accept loop: a UnixListener only produces
                                            // IpcPid peers. Ignore defensively.
                                        }
                                    }
                                }
                                Err(e) =>
                                {
                                    let _ = accept_tx.send(RegistryInput::Event(
                                        NetEvent::Error
                                        {
                                            id:      parent_id.clone(),
                                            message: format!("accept failed: {e}"),
                                        }
                                    )).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                }
                            }
                        }
                    });
                    let srv = ServerHandle
                    {
                        state:          ConnectionState::new_connected(id.clone(), None),
                        accept_task:    task,
                        children:       Default::default(),
                        ipc_path:       Some(path.clone()),
                        disambig_next:  HashMap::new(),
                        base_no_suffix: HashMap::new(),
                    };
                    reg.servers.insert(id.clone(), srv);
                    let _ = display.send(DisplayLine::new(
                        format!("[{id}] listening")
                    )).await;
                    CommandOutcome::ok(format!("listening on {id}"))
                }
                Err(e) => CommandOutcome::err(e.to_string()),
            }
        }
        Command::OpenTcpClient { local_port, remote_host, remote_port } =>
        {
            let id = ConnectionId::TcpClient { local_port };
            if reg.streams.contains_key(&id) || reg.servers.contains_key(&id)
            {
                return CommandOutcome::err(
                    DomainError::IdAlreadyInUse(local_port.to_string()).to_string(),
                );
            }
            let endpoint = Endpoint::Tcp
            {
                local_port,
                remote_host: remote_host.clone(),
                remote_port,
            };
            match connector.connect(endpoint).await
            {
                Ok(stream) =>
                {
                    let peer = PeerInfo::TcpSocket
                    {
                        addr: format!("{remote_host}:{remote_port}"),
                    };
                    let handle = spawn_stream(
                        id.clone(),
                        stream,
                        Some(peer.clone()),
                        None,
                        input_tx.clone(),
                    );
                    reg.insert_stream(handle);
                    let _ = display.send(DisplayLine::new(format!(
                        "[{id}] connected to {remote_host}:{remote_port}"
                    ))).await;
                    CommandOutcome::ok(format!("client {id} connected"))
                }
                Err(e) => CommandOutcome::err(e.to_string()),
            }
        }
        Command::OpenIpcClient { remote_path } =>
        {
            let seq = reg.alloc_ipc_client_seq();
            let id  = ConnectionId::IpcClient { seq };
            // By construction (monotonic seq) this id cannot collide.
            let endpoint = Endpoint::Ipc { remote_path: remote_path.clone() };
            match connector.connect(endpoint).await
            {
                Ok(stream) =>
                {
                    let peer = PeerInfo::IpcRemotePath { path: remote_path.clone() };
                    let handle = spawn_stream(
                        id.clone(),
                        stream,
                        Some(peer),
                        None,
                        input_tx.clone(),
                    );
                    reg.insert_stream(handle);
                    let _ = display.send(DisplayLine::new(format!(
                        "[{id}] connected to {}",
                        remote_path.display()
                    ))).await;
                    CommandOutcome::ok(format!(
                        "ipc client {id} connected to {}",
                        remote_path.display()
                    ))
                }
                Err(e) => CommandOutcome::err(e.to_string()),
            }
        }
        Command::Close { id_token } =>
        {
            let id = match reg.resolve_id(&id_token, default_token_parser)
            {
                Ok(id) => id,
                Err(e) => return CommandOutcome::err(e.to_string()),
            };
            if reg.servers.contains_key(&id)
            {
                // Cascading close (see Registry::drop_server).
                reg.drop_server(&id);
                let _ = display.send(DisplayLine::new(format!("[{id}] CLOSED"))).await;
                CommandOutcome::ok(format!("closed server {id} and all its children"))
            }
            else if reg.streams.contains_key(&id)
            {
                reg.drop_stream(&id);
                let _ = display.send(DisplayLine::new(format!("[{id}] CLOSED"))).await;
                CommandOutcome::ok(format!("closed {id}"))
            }
            else
            {
                CommandOutcome::err(DomainError::UnknownId(id_token).to_string())
            }
        }
        Command::Send { id_token, bytes } =>
        {
            let id = match reg.resolve_id(&id_token, default_token_parser)
            {
                Ok(id) => id,
                Err(e) => return CommandOutcome::err(e.to_string()),
            };
            match reg.send_bytes(&id, &bytes, display).await
            {
                Ok(())  => CommandOutcome::ok(format!("sent {} bytes to {id}", bytes.len())),
                Err(e)  => CommandOutcome::err(e.to_string()),
            }
        }
        Command::Label { id_token, name } =>
        {
            let id = match reg.resolve_id(&id_token, default_token_parser)
            {
                Ok(id) => id,
                Err(e) => return CommandOutcome::err(e.to_string()),
            };
            reg.cmd_label(&id, &name)
        }
        Command::Status =>
        {
            reg.cmd_status()
        }
    }
}

async fn handle_event(
    reg:     &mut Registry,
    ev:      NetEvent,
    display: &mpsc::Sender<DisplayLine>,
)
{
    match ev
    {
        NetEvent::Received { id, bytes } =>
        {
            reg.on_received(&id, &bytes, display).await;
        }
        NetEvent::Closed { id } =>
        {
            reg.on_closed_event(&id, display).await;
        }
        NetEvent::Error { id, message } =>
        {
            reg.on_error_event(&id, &message, display).await;
        }
        NetEvent::Renamed { .. } =>
        {
            // Accept handoffs go through `RegistryInput::Accepted`; Renamed is
            // emitted synchronously from the registry and displayed inline.
        }
    }
}

async fn handle_accepted(
    reg:       &mut Registry,
    parent:    ConnectionId,
    tentative: ConnectionId,
    peer_info: PeerInfo,
    stream:    BoxedStream,
    input_tx:  &mpsc::Sender<RegistryInput>,
    display:   &mpsc::Sender<DisplayLine>,
)
{
    // Resolve final id with disambiguation logic in the registry.
    let (final_id, rename_evt) = match (&parent, &tentative)
    {
        (ConnectionId::TcpServer { port }, ConnectionId::AcceptedTcp { remote_port, .. }) =>
        {
            reg.allocate_accepted_tcp_id(*port, *remote_port)
        }
        (ConnectionId::IpcServer { path }, ConnectionId::AcceptedIpc { pid, .. }) =>
        {
            reg.allocate_accepted_ipc_id(path.clone(), *pid)
        }
        _ =>
        {
            // Mismatched combination: log and drop.
            let _ = display.send(DisplayLine::new(format!(
                "[{parent}] ERROR internal: mismatched accepted id shape"
            ))).await;
            return;
        }
    };

    if let Some(NetEvent::Renamed { new_id, old_id }) = rename_evt
    {
        let _ = display.send(DisplayLine::new(format!(
            "[{new_id}] RENAMED {old_id}"
        ))).await;
    }

    let handle = spawn_stream(
        final_id.clone(),
        stream,
        Some(peer_info.clone()),
        Some(parent.clone()),
        input_tx.clone(),
    );
    reg.insert_stream(handle);

    let _ = display.send(DisplayLine::new(format!(
        "[{final_id}] ACCEPT peer={}",
        peer_info.render()
    ))).await;
}

/// Spawn a read task and build the `StreamHandle`.
///
/// The read task is strictly I/O: it reads up to 64 KiB, forwards bytes, and
/// forwards `Closed`/`Error` lifecycle events. No business logic here.
fn spawn_stream(
    id:        ConnectionId,
    stream:    BoxedStream,
    peer_info: Option<PeerInfo>,
    parent:    Option<ConnectionId>,
    input_tx:  mpsc::Sender<RegistryInput>,
) -> StreamHandle
{
    let (mut reader, writer) = tokio::io::split(stream);
    let writer_box: BoxedStream = Box::new(WriterHalf { writer });

    let read_id = id.clone();
    let read_tx = input_tx.clone();
    let read_task = tokio::spawn(async move
    {
        let mut buf = vec![0_u8; 64 * 1024];
        loop
        {
            match reader.read(&mut buf).await
            {
                Ok(0) =>
                {
                    let _ = read_tx.send(RegistryInput::Event(NetEvent::Closed
                    {
                        id: read_id.clone(),
                    })).await;
                    break;
                }
                Ok(n) =>
                {
                    let _ = read_tx.send(RegistryInput::Event(NetEvent::Received
                    {
                        id:    read_id.clone(),
                        bytes: buf[..n].to_vec(),
                    })).await;
                }
                Err(e) =>
                {
                    let _ = read_tx.send(RegistryInput::Event(NetEvent::Error
                    {
                        id:      read_id.clone(),
                        message: e.to_string(),
                    })).await;
                    let _ = read_tx.send(RegistryInput::Event(NetEvent::Closed
                    {
                        id: read_id.clone(),
                    })).await;
                    break;
                }
            }
        }
    });

    let state = ConnectionState::new_connected(id, peer_info);
    StreamHandle
    {
        state,
        stream: writer_box,
        read_task,
        parent,
    }
}

struct WriterHalf
{
    writer: tokio::io::WriteHalf<BoxedStream>,
}

impl tokio::io::AsyncRead for WriterHalf
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx:  &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>>
    {
        // Write-only half: reads are not supported. Return EOF.
        std::task::Poll::Ready(Ok(()))
    }
}

impl tokio::io::AsyncWrite for WriterHalf
{
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx:       &mut std::task::Context<'_>,
        buf:      &[u8],
    ) -> std::task::Poll<std::io::Result<usize>>
    {
        std::pin::Pin::new(&mut self.writer).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx:       &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>>
    {
        std::pin::Pin::new(&mut self.writer).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx:       &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>>
    {
        std::pin::Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

fn parse_remote_port(sock_str: &str) -> Option<u16>
{
    // `addr.to_string()` on a SocketAddr yields `ip:port` (or `[ipv6]:port`).
    let s = sock_str;
    if let Some(rest) = s.strip_suffix(|c: char| c.is_ascii_digit())
    {
        // fast path: find last ':'.
        let _ = rest; // silence unused
    }
    let colon = s.rfind(':')?;
    s[colon + 1..].parse::<u16>().ok()
}