// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Bot.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;
use log::{error, info};

use tokio::{
    signal,
    sync::{broadcast, mpsc},
};

mod client;
pub use client::Strategy;

/// Bot clients configuration.
#[derive(Debug)]
pub struct Config {
    /// Number of clients to run.
    pub clients: u8,
    /// The server listening address.
    pub host: String,
    /// The server listening port.
    pub port: u16,
}

static NICKNAMES: &[&str] = &["Alice", "Bob", "Carol", "Dave", "Frank", "Mike"];

/// Runs clients given a config and a strategy factory called for each client.
pub async fn run<F, S>(config: Config, factory: F) -> Result<()>
where
    F: Fn() -> S,
    S: Strategy,
{
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp_millis()
        .init();

    let (shutdown_broadcast_tx, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    for idx in 0..config.clients {
        let mut client = client::Client::new(
            factory(),
            NICKNAMES[idx as usize].to_string(),
            &config.host,
            config.port,
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
