//! Hand-written CLI argument parser. No `clap`.
//!
//! Syntax:
//!   wiredock [--log-level=trace|info|warn|error] [--help]
//!
//! Unknown flags are an error. `--help` prints usage to stderr and requests
//! an exit (returned via `CliArgs::help_requested`).

use std::env;

#[derive(Debug)]
pub struct CliArgs
{
    pub log_level:       u8,
    pub help_requested:  bool,
}

#[derive(Debug)]
pub enum CliError
{
    Unknown(String),
    BadValue(String),
}

impl std::fmt::Display for CliError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        match self
        {
            Self::Unknown(s)  => write!(f, "unknown argument: {s}"),
            Self::BadValue(s) => write!(f, "invalid value: {s}"),
        }
    }
}

impl std::error::Error for CliError {}

impl CliArgs
{
    pub fn parse_env() -> Result<Self, CliError>
    {
        let argv: Vec<String> = env::args().skip(1).collect();
        Self::parse_slice(&argv)
    }

    pub fn parse_slice(argv: &[String]) -> Result<Self, CliError>
    {
        let mut log_level      = 1_u8; // Info
        let mut help_requested = false;

        for a in argv
        {
            if a == "--help" || a == "-h"
            {
                help_requested = true;
                continue;
            }
            if let Some(v) = a.strip_prefix("--log-level=")
            {
                log_level = match v
                {
                    "trace" => 0,
                    "info"  => 1,
                    "warn"  => 2,
                    "error" => 3,
                    _       => return Err(CliError::BadValue(a.clone())),
                };
                continue;
            }
            return Err(CliError::Unknown(a.clone()));
        }

        Ok(Self { log_level, help_requested })
    }

    pub fn usage() -> &'static str
    {
        "\
usage: wiredock [--log-level=trace|info|warn|error] [--help]

Interactive CLI to test software over TCP and Unix-domain sockets.
Type commands at the prompt. See docs for the full command set.
"
    }
}