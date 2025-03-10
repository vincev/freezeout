// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout CLI client.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;
use clap::Parser;

pub mod network;

#[derive(Debug, Parser)]
struct Cli {
    /// The server listening address.
    #[clap(long, short, default_value = "127.0.0.1")]
    address: String,
    /// The server listening port.
    #[clap(long, short, default_value_t = 9871)]
    port: u16,
    /// The configuration storage key.
    #[clap(long, short, default_value = "default")]
    storage: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut net = network::Network::new();

    net.connect(&cli.address, cli.port).await?;
    net.shutdown().await;

    Ok(())
}
