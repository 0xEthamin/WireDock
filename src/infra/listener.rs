//! Tokio-backed `Listener` / `AcceptLoop` for TCP and IPC servers.

use std::path::PathBuf;

use tokio::net::{TcpListener, UnixListener};

use crate::domain::endpoint::ServerEndpoint;
use crate::domain::error::DomainError;
use crate::domain::event::PeerInfo;
use crate::domain::io::BoxedStream;
use crate::domain::transport::{AcceptLoop, AcceptedStream, Listener};
use crate::infra::error::InfraError;
use crate::infra::fs_cleanup;
use crate::infra::peercred;

/// Concrete listener factory.
#[derive(Default, Clone, Copy)]
pub struct TokioListener;

impl Listener for TokioListener
{
    async fn bind(
        &self,
        endpoint: ServerEndpoint,
    ) -> Result<Box<dyn AcceptLoop>, DomainError>
    {
        match endpoint
        {
            ServerEndpoint::Tcp { port } =>
            {
                let listener = TcpListener::bind(("0.0.0.0", port))
                    .await
                    .map_err(|e|
                    {
                        if e.kind() == std::io::ErrorKind::AddrInUse
                        {
                            InfraError::AddressInUse(port.to_string())
                        }
                        else
                        {
                            InfraError::Io(e)
                        }
                    })?;
                let b: Box<dyn AcceptLoop> = Box::new(TcpAcceptLoop { inner: listener });
                Ok(b)
            }
            ServerEndpoint::Ipc { path } =>
            {
                // Remove any stale socket file from a previous crashed run.
                fs_cleanup::unlink_if_stale(&path);
                let listener = UnixListener::bind(&path).map_err(|e|
                {
                    if e.kind() == std::io::ErrorKind::AddrInUse
                    {
                        InfraError::AddressInUse(path.display().to_string())
                    }
                    else
                    {
                        InfraError::Io(e)
                    }
                })?;
                let b: Box<dyn AcceptLoop> = Box::new(IpcAcceptLoop
                {
                    inner: listener,
                    path,
                });
                Ok(b)
            }
        }
    }
}

struct TcpAcceptLoop
{
    inner: TcpListener,
}

impl AcceptLoop for TcpAcceptLoop
{
    fn accept(
        &mut self,
    ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AcceptedStream, DomainError>> + Send + '_>,
        >
    {
        Box::pin(async move
        {
            let (stream, addr) = self.inner.accept().await.map_err(InfraError::Io)?;
            let peer_info = PeerInfo::TcpSocket { addr: addr.to_string() };
            Ok(AcceptedStream
            {
                stream: Box::new(stream) as BoxedStream,
                peer_info,
            })
        })
    }
}

struct IpcAcceptLoop
{
    inner: UnixListener,
    #[allow(dead_code)] // kept for symmetry / future telemetry
    path:  PathBuf,
}

impl AcceptLoop for IpcAcceptLoop
{
    fn accept(
        &mut self,
    ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AcceptedStream, DomainError>> + Send + '_>,
        >
    {
        Box::pin(async move
        {
            let (stream, _addr) = self.inner.accept().await.map_err(InfraError::Io)?;
            // SO_PEERCRED is mandatory for id derivation. If it fails, reject
            // the accept with a clear error so the caller can emit an ERROR
            // event and drop the socket. No silent fallback.
            let pid = peercred::peer_pid(&stream)
                .map_err(|e| InfraError::PeerCred(e.to_string()))?;
            let peer_info = PeerInfo::IpcPid { pid };
            Ok(AcceptedStream
            {
                stream: Box::new(stream) as BoxedStream,
                peer_info,
            })
        })
    }
}