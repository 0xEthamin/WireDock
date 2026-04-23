//! `SO_PEERCRED` helper.
//!
//! We use `tokio::net::UnixStream::peer_cred`, which under the hood does the
//! `getsockopt(SO_PEERCRED)` call on Linux. The PID is mandatory in wiredock
//! because it is the sole discriminant for accepted-IPC identifiers.

use std::io;

use tokio::net::UnixStream;

/// Return the peer PID for a freshly accepted Unix stream.
pub fn peer_pid(stream: &UnixStream) -> io::Result<i32>
{
    let cred = stream.peer_cred()?;
    cred.pid().ok_or_else(|| io::Error::other(
        "peer_cred returned no pid",
    ))
}