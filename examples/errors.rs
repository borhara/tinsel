//! Turning a Rust error into a readable report, with its whole cause chain.
//!
//!   cargo run --example errors

use std::fmt;

use tinsel::prelude::*;

/// An error that wraps another, like most real ones do.
#[derive(Debug)]
struct ConfigError {
    path: String,
    source: std::io::Error,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "couldn't read {}", self.path)
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn load_config() -> Result<String, ConfigError> {
    let path = "app.toml".to_string();
    std::fs::read_to_string("/definitely/not/here/app.toml").map_err(|source| ConfigError { path, source })
}

fn main() {
    if let Err(e) = load_config() {
        Report::from_error("couldn't start", &e)
            .hint("create app.toml, or pass --config <path>")
            .eprint();
        std::process::exit(1);
    }
}
