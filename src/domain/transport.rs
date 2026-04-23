//! Transport abstractions: `Connector` for outgoing connections, `Listener`
//! for servers, and `AcceptLoop` for the accept side.
//!
//! All use native `async fn` / return-position-impl-future to avoid
//! `async_trait`, as required by the spec.

use std::future::Future;

use crate::domain::endpoint::{Endpoint, ServerEndpoint};
use crate::domain::event::PeerInfo;
use crate::domain::io::BoxedStream;

/// Outgoing connector. Tests substitute this with an in-memory mock.
pub trait Connector: Send + Sync
{
    /// Establish a client connection.
    fn connect(
        &self,
        endpoint: Endpoint,
    ) -> impl Future<Output = Result<BoxedStream, crate::domain::DomainError>> + Send;
}

/// Listener factory. The concrete return type is erased behind `AcceptLoop`.
pub trait Listener: Send + Sync
{
    /// Bind a server endpoint and return an erased accept loop handle.
    fn bind(
        &self,
        endpoint: ServerEndpoint,
    ) -> impl Future<Output = Result<Box<dyn AcceptLoop>, crate::domain::DomainError>> + Send;
}

/// Runtime-accept abstraction. Each call yields one accepted stream + peer.
///
/// The domain does not own the accept loop; the runtime drives it in a
/// dedicated task and forwards handoffs to the registry via `RegistryInput`.
pub trait AcceptLoop: Send
{
    /// Accept one connection.
    fn accept(
        &mut self,
    ) -> std::pin::Pin<
        Box<
            dyn Future<Output = Result<AcceptedStream, crate::domain::DomainError>>
                + Send
                + '_,
        >,
    >;
}


/// Outcome of one accept.
pub struct AcceptedStream
{
    pub stream:    BoxedStream,
    pub peer_info: PeerInfo,
}