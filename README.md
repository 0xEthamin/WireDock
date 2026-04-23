# wiredock

**wiredock** is an interactive, general-purpose debugging CLI tool designed to test software over TCP and Unix-domain sockets (IPC). 

It allows you to open listeners, connect to external services, and exchange raw hexadecimal data on the fly from a clean, interactive REPL. Asynchronous network events (accepted connections, incoming data, closures) are displayed in real-time without ever corrupting your current input prompt.

## Features

- **Multi-Transport:** Native support for TCP and Unix-domain sockets (IPC).
- **Server & Client:** Act as a server accepting multiple clients, or as a client connecting to external targets.
- **Interactive REPL:** Built on `reedline` for a smooth, terminal-friendly experience with asynchronous background event printing.
- **Stable Identifiers:** Every connection gets a deterministic, predictable ID (based on ports, paths, and PIDs).
- **Labeling System:** Assign human-readable aliases to raw connection IDs.
- **Raw Hex Data:** Send arbitrary byte payloads effortlessly using a permissive hex syntax.
- **Broadcast Semantics:** Sending data to a server ID automatically broadcasts it to all currently accepted clients.
- **Clean Shutdown:** Deterministic teardown on `Ctrl+C`. All network resources and IPC socket files are explicitly cleaned up.

---

## Installation

Ensure you have Rust installed (MSRV: 1.95).

```bash
git clone <your-repo>/wiredock
cd wiredock
cargo build --release
```

Run the executable:
```bash
./target/release/wiredock
```

### CLI Arguments

```bash
usage: wiredock [--log-level=trace|info|warn|error] [--help]
```

---

## Quick Start

```text
# Open a TCP server on port 3000
〉open server 3000
OK listening on 3000

# Open an outgoing client from local port 10016 to a remote SSH server
〉open client 10016 192.168.1.10:22
OK client 10016 connected

# Check current registry state
〉status
OK CLIENTS
  10016 -> 192.168.1.10:22    [CONNECTED]

SERVERS
  3000
    (none)

# Assign an alias to the client connection
〉label 10016 ssh_target
OK labeled 10016 as ssh_target

# Send 5 raw bytes (0x48 0x65 0x6C 0x6C 0x6F) to the server
〉ssh_target 48 65 6C 6C 6F
OK sent 5 bytes to 10016
[ssh_target] RECV 53 53 48 2D 32 2E 30

# Close the connection
〉close ssh_target
[ssh_target] CLOSED
OK closed 10016
```

---

## Identifier System

To interact with a connection, you must use its **Connection ID**. `wiredock` enforces a strict and predictable naming convention:

### TCP IDs
- **Server:** Its local port (e.g., `3000`).
- **Client:** Its local bound port (e.g., `10016`).
- **Accepted Connection:** `<server_port>.<remote_port>`. If multiple clients connect from the same remote port (different IPs), a disambiguation suffix is added (e.g., `3000.10015#1`, `3000.10015#2`).

### IPC (Unix Sockets) IDs
- **Server:** Its local path (e.g., `/tmp/server.sock`).
- **Client:** Its local bound path (e.g., `/tmp/client.sock`).
- **Accepted Connection:** `<server_path>.<pid>`. Derived via `SO_PEERCRED` (e.g., `/tmp/server.sock.12345`). Suffixes are added upon PID collision.

---

## Command Reference

### Connection Management

| Command | Description |
| :--- | :--- |
| `open server <port>` | Start a TCP listener on the given port. |
| `open ipc server <path>` | Start an IPC listener. Removes stale socket files automatically. |
| `open client <local_port> <host>:<port>` | Open a TCP client bound to `local_port`, connecting to `host:port`. |
| `open ipc client <local_path> <remote_path>` | Open an IPC client bound to `local_path`, connecting to `remote_path`. |
| `close <id>` | Close a connection. Closing a server triggers a **cascading close** of all its accepted children and removes the socket file (if IPC). |

### Sending Data

```text
<id> <hex bytes...>
```
Sends raw bytes over the specified connection.
- **Syntax:** Hexadecimal digits separated by ASCII whitespace (spaces/tabs). Case-insensitive. No `0x` prefix.
- **Example:** `3000.10015 0A 1B 2C 3D`
- **Targeting a Stream:** Writes strictly to that client/accepted connection.
- **Targeting a Server:** **Broadcasts** the payload to *every* active client currently accepted by that server.

### Utilities

| Command | Description |
| :--- | :--- |
| `label <id> <name>` | Assign a human-readable alias to a connection. The label can be used in place of the ID anywhere. Labels are freed automatically on close. |
| `status` | Prints the complete state of the connection registry, separating isolated clients and servers with their children. |

---

## Architecture & Technical Choices

- **Clean Architecture:** Strict unidirectional dependency flow (`domain` <- `infra` / `repl`). The core domain is entirely agnostic of `tokio::net` transports.
- **Zero-cost Abstractions:** I/O goes through a dynamic `BoxedStream` blanket implementation, avoiding heavy async-trait overhead while keeping the domain completely decoupled from concrete socket types.
- **Lock-free Registry:** The connection state is owned exclusively by a single Tokio task communicating via MPSC channels. No global `Mutex` is used for the business logic.
- **No Heavy Dependencies:** Designed for critical environments. Omits large crates like `anyhow` or `clap` in favor of hand-written, easily auditable parsing and typed `thiserror` enums.

## License

This project is licensed under the **AGPL-3.0-only** License. See the `Cargo.toml` for details.