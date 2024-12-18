// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker server entry point.
use anyhow::{anyhow, Result};
use log::{error, info};
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    signal,
    sync::{broadcast, mpsc},
    time::{self, Duration},
};

use crate::connection;

/// Networking config.
#[derive(Debug)]
pub struct Config {
    /// The server listening address.
    pub address: String,
    /// The server listening port.
    pub port: u16,
}

/// The server that handles client connection and state.
#[derive(Debug)]
struct Server {
    /// The server listener.
    listener: TcpListener,
    /// Shutdown notification channel.
    shutdown_broadcast_tx: broadcast::Sender<()>,
    /// Shutdown sender cloned by each connection.
    shutdown_complete_tx: mpsc::Sender<()>,
}

/// Server entry point.
pub async fn run(config: Config) -> Result<()> {
    let addr = format!("{}:{}", config.address, config.port);
    info!("Starting server listening on {}", addr);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow!("Tcp listener bind error: {e}"))?;

    let shutdown_signal = signal::ctrl_c();
    let (shutdown_broadcast_tx, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let mut server = Server {
        listener,
        shutdown_broadcast_tx,
        shutdown_complete_tx,
    };

    tokio::select! {
        res = server.run() => {
            res.map_err(|e| anyhow!("Tcp listener accept error: {e}"))?;
        }
        _ = shutdown_signal => {
            info!("Received shutdown signal...");
        }
    }

    // Wait for all connection to shutdown.
    let Server {
        shutdown_broadcast_tx,
        shutdown_complete_tx,
        ..
    } = server;

    // Notify all connections to start shutdown then wait for all connections to
    // terminate and drop their shutdown channel.
    drop(shutdown_broadcast_tx);
    drop(shutdown_complete_tx);
    let _ = shutdown_complete_rx.recv().await;

    Ok(())
}

impl Server {
    /// Runs the server.
    async fn run(&mut self) -> Result<()> {
        loop {
            let (socket, addr) = self.accept_with_retry().await?;
            info!("Accepted connection from {addr}");

            let mut handler = Handler {
                shutdown_broadcast_rx: self.shutdown_broadcast_tx.subscribe(),
                _shutdown_complete_tx: self.shutdown_complete_tx.clone(),
            };

            // Spawn a task to handle connection messages.
            tokio::spawn(async move {
                if let Err(err) = handler.run(socket, addr).await {
                    error!("Connection to {addr} error {err}");
                }
            });
        }
    }

    /// Accepts a connection with retries.
    async fn accept_with_retry(&self) -> Result<(TcpStream, SocketAddr)> {
        let mut retry = 0;
        loop {
            match self.listener.accept().await {
                Ok((socket, addr)) => {
                    return Ok((socket, addr));
                }
                Err(err) => {
                    if retry == 5 {
                        return Err(err.into());
                    }
                }
            }

            time::sleep(Duration::from_secs(1 << retry)).await;
            retry += 1;
        }
    }
}

/// Client connection handler.
struct Handler {
    /// Channel for listening shutdown notification.
    shutdown_broadcast_rx: broadcast::Receiver<()>,
    /// Sender that drops when this connection is done.
    _shutdown_complete_tx: mpsc::Sender<()>,
}

impl Handler {
    /// Handle connection messages.
    async fn run(&mut self, socket: TcpStream, addr: SocketAddr) -> Result<()> {
        let mut conn = connection::accept_async(socket).await?;

        loop {
            tokio::select! {
                _ = self.shutdown_broadcast_rx.recv() => {
                    break;
                }
                res = conn.recv() => match res {
                    Some(Ok(msg)) => {
                        info!("Received message from {addr}: {msg:?}");
                    },
                    Some(Err(err)) => {
                        error!("Connection to {addr} error {err}");
                        break;
                    },
                    None => break,
                },
            }
        }

        conn.close().await;

        info!("Connection to {addr} closed");

        Ok(())
    }
}
