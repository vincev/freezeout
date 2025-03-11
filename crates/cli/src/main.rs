// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout CLI client.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;
use clap::Parser;

use freezeout_core::{crypto::SigningKey, message::Message};

pub mod network;
pub mod terminal;

#[derive(Debug, Parser)]
struct Cli {
    /// This client nickname.
    #[clap(long, short)]
    nickname: String,
    /// The server listening address.
    #[clap(long, short, default_value = "127.0.0.1")]
    address: String,
    /// The server listening port.
    #[clap(long, short, default_value_t = 9871)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Connect to the server before starting the terminal.
    let mut net = network::Network::new(SigningKey::default());
    net.connect(&cli.address, cli.port).await?;

    // Request to join server with the given nickname.
    net.send(Message::JoinServer {
        nickname: cli.nickname.to_string(),
    })
    .await?;

    // Wait for ServerJoined message or exit.
    let msg = net.recv().await?;

    if let Message::ServerJoined { nickname, .. } = msg.message() {
        terminal::run(net, nickname.to_string()).await?;
    }

    Ok(())
}
