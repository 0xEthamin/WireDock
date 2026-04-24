//! Tokio-backed `Connector` for outgoing TCP and IPC client connections.

use std::net::SocketAddr;
use std::path::PathBuf;

use tokio::net::{TcpSocket, UnixStream};

use crate::domain::endpoint::Endpoint;
use crate::domain::error::DomainError;
use crate::domain::io::BoxedStream;
use crate::domain::transport::Connector;
use crate::infra::error::InfraError;

/// Concrete implementation using `tokio::net`.
#[derive(Default, Clone, Copy)]
pub struct TokioConnector;

impl Connector for TokioConnector
{
    async fn connect(
        &self,
        endpoint: Endpoint,
    ) -> Result<BoxedStream, DomainError>
    {
        match endpoint
        {
            Endpoint::Tcp { local_port, remote_host, remote_port } =>
            {
                connect_tcp(local_port, &remote_host, remote_port).await
            }
            Endpoint::Ipc { remote_path } =>
            {
                connect_ipc(remote_path).await
            }
        }
    }
}

async fn connect_tcp(
    local_port:  u16,
    remote_host: &str,
    remote_port: u16,
) -> Result<BoxedStream, DomainError>
{
    // Resolve remote, bind local port, then connect.
    let remote_str = format!("{remote_host}:{remote_port}");
    let mut addrs  = tokio::net::lookup_host(remote_str.as_str())
        .await
        .map_err(InfraError::Io)?;
    let remote: SocketAddr = addrs
        .next()
        .ok_or_else(|| InfraError::InvalidEndpoint(remote_str.clone()))?;

    let socket = if remote.is_ipv6()
    {
        TcpSocket::new_v6().map_err(InfraError::Io)?
    }
    else
    {
        TcpSocket::new_v4().map_err(InfraError::Io)?
    };
    let local: SocketAddr = if remote.is_ipv6()
    {
        format!("[::]:{local_port}").parse().map_err(|e|
            InfraError::InvalidEndpoint(format!("local ipv6: {e}")))?
    }
    else
    {
        format!("0.0.0.0:{local_port}").parse().map_err(|e|
            InfraError::InvalidEndpoint(format!("local ipv4: {e}")))?
    };
    socket.bind(local).map_err(|e|
    {
        if e.kind() == std::io::ErrorKind::AddrInUse
        {
            InfraError::AddressInUse(local_port.to_string())
        }
        else
        {
            InfraError::Io(e)
        }
    })?;

    let stream = socket.connect(remote).await.map_err(InfraError::Io)?;
    Ok(Box::new(stream) as BoxedStream)
}

async fn connect_ipc(remote_path: PathBuf) -> Result<BoxedStream, DomainError>
{
    let stream = UnixStream::connect(&remote_path)
        .await
        .map_err(InfraError::Io)?;
    Ok(Box::new(stream) as BoxedStream)
}