//! Line parser: raw REPL string -> `Command`.
//!
//! Pure, no I/O. Only this module and `args.rs` construct domain commands
//! from strings. Everywhere else manipulates typed values.

use std::path::PathBuf;

use crate::domain::command::Command;
use crate::domain::hex;
use crate::repl::error::ReplError;

/// Parse a single REPL line. Empty input returns `Ok(None)`.
pub fn parse_line(line: &str) -> Result<Option<Command>, ReplError>
{
    let line = line.trim();
    if line.is_empty()
    {
        return Ok(None);
    }
    let tokens: Vec<&str> = line.split([' ', '\t'])
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty()
    {
        return Ok(None);
    }

    match tokens[0]
    {
        "open" =>
        {
            parse_open(&tokens[1..])
        }
        "close" =>
        {
            if tokens.len() != 2
            {
                return Err(ReplError::Parse(
                    "usage: close <id>".to_string(),
                ));
            }
            Ok(Some(Command::Close { id_token: tokens[1].to_string() }))
        }
        "label" =>
        {
            if tokens.len() != 3
            {
                return Err(ReplError::Parse(
                    "usage: label <id> <name>".to_string(),
                ));
            }
            Ok(Some(Command::Label
            {
                id_token: tokens[1].to_string(),
                name:     tokens[2].to_string(),
            }))
        }
        "status" =>
        {
            if tokens.len() != 1
            {
                return Err(ReplError::Parse("usage: status".to_string()));
            }
            Ok(Some(Command::Status))
        }
        _ =>
        {
            // Send syntax: `<id> <hex...>`
            if tokens.len() < 2
            {
                return Err(ReplError::Parse(
                    "expected `<id> <hex bytes...>` or a known command".to_string(),
                ));
            }
            let bytes = hex::parse_hex_tokens(&tokens[1..])?;
            Ok(Some(Command::Send
            {
                id_token: tokens[0].to_string(),
                bytes,
            }))
        }
    }
}

fn parse_open(args: &[&str]) -> Result<Option<Command>, ReplError>
{
    if args.is_empty()
    {
        return Err(ReplError::Parse(
            "usage: open server <port> | open ipc server <path> | open client <local_port> <host>:<port> | open ipc client <local_path> <remote_path>"
                .to_string(),
        ));
    }
    match args[0]
    {
        "server" =>
        {
            if args.len() != 2
            {
                return Err(ReplError::Parse(
                    "usage: open server <port>".to_string(),
                ));
            }
            let port: u16 = args[1].parse().map_err(|_|
            {
                ReplError::Parse(format!("invalid port: {}", args[1]))
            })?;
            Ok(Some(Command::OpenTcpServer { port }))
        }
        "client" =>
        {
            if args.len() != 3
            {
                return Err(ReplError::Parse(
                    "usage: open client <local_port> <host>:<port>".to_string(),
                ));
            }
            let local_port: u16 = args[1].parse().map_err(|_|
            {
                ReplError::Parse(format!("invalid local port: {}", args[1]))
            })?;
            let (host, remote_port) = split_host_port(args[2])?;
            Ok(Some(Command::OpenTcpClient
            {
                local_port,
                remote_host: host,
                remote_port,
            }))
        }
        "ipc" =>
        {
            parse_open_ipc(&args[1..])
        }
        other =>
        {
            Err(ReplError::Parse(format!("unknown open variant: {other}")))
        }
    }
}

fn parse_open_ipc(args: &[&str]) -> Result<Option<Command>, ReplError>
{
    if args.is_empty()
    {
        return Err(ReplError::Parse(
            "usage: open ipc server <path> | open ipc client <remote_path>"
                .to_string(),
        ));
    }
    match args[0]
    {
        "server" =>
        {
            if args.len() != 2
            {
                return Err(ReplError::Parse(
                    "usage: open ipc server <path>".to_string(),
                ));
            }
            Ok(Some(Command::OpenIpcServer
            {
                path: PathBuf::from(args[1]),
            }))
        }
        "client" =>
        {
            if args.len() != 2
            {
                return Err(ReplError::Parse(
                    "usage: open ipc client <remote_path>".to_string(),
                ));
            }
            Ok(Some(Command::OpenIpcClient
            {
                remote_path: PathBuf::from(args[1]),
            }))
        }
        other =>
        {
            Err(ReplError::Parse(format!("unknown ipc variant: {other}")))
        }
    }
}

fn split_host_port(s: &str) -> Result<(String, u16), ReplError>
{
    // Support IPv6 literal in brackets: `[::1]:22`.
    if let Some(rest) = s.strip_prefix('[')
    {
        let end = rest.find(']').ok_or_else(||
            ReplError::Parse(format!("malformed ipv6 literal: {s}")))?;
        let host = &rest[..end];
        let after = &rest[end + 1..];
        let port_str = after.strip_prefix(':').ok_or_else(||
            ReplError::Parse(format!("missing port after ipv6 literal: {s}")))?;
        let port: u16 = port_str.parse().map_err(|_|
            ReplError::Parse(format!("invalid port: {port_str}")))?;
        return Ok((host.to_string(), port));
    }
    let (host, port_str) = s.rsplit_once(':').ok_or_else(||
        ReplError::Parse(format!("expected host:port, got {s:?}")))?;
    let port: u16 = port_str.parse().map_err(|_|
        ReplError::Parse(format!("invalid port: {port_str}")))?;
    Ok((host.to_string(), port))
}