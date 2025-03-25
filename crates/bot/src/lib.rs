// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Bot.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;
use clap::Parser;
use log::{error, info};

use tokio::{
    signal,
    sync::{broadcast, mpsc},
};

mod client;
pub use client::Strategy;

#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
struct Cli {
    /// Number of clients to run.
    #[clap(long, short, value_parser = clap::value_parser!(u8).range(1..=5))]
    clients: u8,
    /// The server listening address.
    #[clap(long, short, default_value = "127.0.0.1")]
    host: String,
    /// The server listening port.
    #[clap(long, short, default_value_t = 9871)]
    port: u16,
    /// Help long flag.
    #[clap(long, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,
}

static NICKNAMES: &[&str] = &["Alice", "Bob", "Carol", "Dave", "Frank", "Mike"];

/// Run players bot clients.
pub async fn run<F, S>(factory: F) -> Result<()>
where
    F: Fn() -> S,
    S: Strategy,
{
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    let (shutdown_broadcast_tx, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    for idx in 0..cli.clients {
        let mut client = client::Client::new(
            factory(),
            NICKNAMES[idx as usize].to_string(),
            &cli.host,
            cli.port,
            shutdown_broadcast_tx.subscribe(),
            shutdown_complete_tx.clone(),
        )
        .await?;

        tokio::spawn(async move {
            if let Err(err) = client.run().await {
                error!("Client {idx} error: {err}");
            }

            info!("Client {idx} connection closed");
        });
    }

    let _ = signal::ctrl_c().await;
    info!("Received Ctrl-c signal");

    // Signal clients to shutdown and wait for tasks to complete.
    drop(shutdown_broadcast_tx);
    drop(shutdown_complete_tx);
    let _ = shutdown_complete_rx.recv().await;

    Ok(())
}
