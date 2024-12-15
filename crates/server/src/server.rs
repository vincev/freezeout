// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker server entry point.
use anyhow::{anyhow, Result};
use log::info;
use tokio::net::TcpListener;

/// Networking config.
#[derive(Debug)]
pub struct Config {
    /// The server listening address.
    pub address: String,
    /// The server listening port.
    pub port: u16,
}

/// Server entry point.
pub async fn run(config: Config) -> Result<()> {
    let addr = format!("{}:{}", config.address, config.port);
    info!("Starting server listening on {}", addr);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow!("Tcp listener bind error: {e}"))?;

    let (_socket, peer) = listener
        .accept()
        .await
        .map_err(|e| anyhow!("Tcp listener accept error: {e}"))?;

    info!("Accepted connection from {peer:?}");

    Ok(())
}
