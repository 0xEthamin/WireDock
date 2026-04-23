//! I/O trait aliases for the domain layer.
//!
//! Everything upstream of the registry manipulates `BoxedStream`. Concrete
//! transports (TCP, IPC) satisfy the blanket impl automatically.

use tokio::io::{AsyncRead, AsyncWrite};

/// Trait alias for any bidirectional byte stream the domain can speak to.
pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> AsyncStream for T {}

/// Boxed, erased stream used in registry state.
pub type BoxedStream = Box<dyn AsyncStream>;