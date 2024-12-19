// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker server entry point.
use anyhow::{anyhow, bail, Result};
use log::{error, info};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    signal,
    sync::{broadcast, mpsc},
    time::{self, Duration},
};

use freezeout_core::{
    crypto::{PlayerId, SigningKey},
    message::{Message, SignedMessage},
};

use crate::{
    connection::{self, EncryptedConnection},
    table::Table,
};

/// Networking config.
#[derive(Debug)]
pub struct Config {
    /// The server listening address.
    pub address: String,
    /// The server listening port.
    pub port: u16,
    /// The number of tables on this server.
    pub tables: usize,
    /// The number of seats per table.
    pub seats: usize,
}

/// The server that handles client connection and state.
#[derive(Debug)]
struct Server {
    /// The tables on this server.
    tables: Arc<TablesSet>,
    /// The server signing key shared by all connections.
    sk: Arc<SigningKey>,
    /// The server listener.
    listener: TcpListener,
    /// Shutdown notification channel.
    shutdown_broadcast_tx: broadcast::Sender<()>,
    /// Shutdown sender cloned by each connection.
    shutdown_complete_tx: mpsc::Sender<()>,
}

/// The tables on this server.
#[derive(Debug)]
struct TablesSet(Vec<Arc<Table>>);

/// Client connection handler.
struct Handler {
    /// The table for this connection.
    table: Option<Arc<Table>>,
    /// This handler player id.
    player_id: PlayerId,
    /// The tables on this server.
    tables: Arc<TablesSet>,
    /// The server signing key shared by all connections.
    sk: Arc<SigningKey>,
    /// Channel for listening shutdown notification.
    shutdown_broadcast_rx: broadcast::Receiver<()>,
    /// Sender that drops when this connection is done.
    _shutdown_complete_tx: mpsc::Sender<()>,
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
        tables: Arc::new(TablesSet::new(config.tables, config.seats)),
        sk: Arc::new(SigningKey::default()),
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
                table: None,
                player_id: PlayerId::default(),
                sk: self.sk.clone(),
                tables: self.tables.clone(),
                shutdown_broadcast_rx: self.shutdown_broadcast_tx.subscribe(),
                _shutdown_complete_tx: self.shutdown_complete_tx.clone(),
            };

            // Spawn a task to handle connection messages.
            tokio::spawn(async move {
                if let Err(err) = handler.run(socket, addr).await {
                    error!("Connection to {addr} {err}");
                }

                info!("Connection to {addr} closed");
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

impl TablesSet {
    /// Creates a new table set.
    fn new(tables: usize, seats: usize) -> Self {
        Self((0..tables).map(|_| Arc::new(Table::new(seats))).collect())
    }

    /// Try to join a table on this set.
    ///
    /// Returns None if no table is available.
    fn join_table(&self, player_id: &PlayerId, nickname: &str) -> Option<Arc<Table>> {
        for table in &self.0 {
            if table.join(player_id, nickname).is_ok() {
                return Some(table.clone());
            }
        }

        None
    }
}

impl Handler {
    /// Handle connection messages.
    async fn run(&mut self, socket: TcpStream, addr: SocketAddr) -> Result<()> {
        let mut conn = connection::accept_async(socket).await?;

        let res = loop {
            tokio::select! {
                _ = self.shutdown_broadcast_rx.recv() => {
                    break Ok(());
                }
                res = conn.recv() => match res {
                    Some(Ok(msg)) => {
                        let res = self.handle_message(&mut conn, msg).await;
                        if res.is_err() {
                            break res;
                        }
                    },
                    Some(Err(err)) => break Err(err),
                    None => break Ok(()),
                },
            }
        };

        conn.close().await;

        if let Some(table) = &self.table {
            table.leave(&self.player_id);
        }

        res
    }

    async fn handle_message(
        &mut self,
        conn: &mut EncryptedConnection,
        msg: SignedMessage,
    ) -> Result<()> {
        let player_id = msg.player_id();
        match msg.to_message() {
            Message::JoinTable(nickname) => {
                if self.table.is_none() {
                    self.table = self.tables.join_table(&player_id, &nickname);
                    self.player_id = player_id;
                }

                if self.table.is_none() {
                    // Notify the client that there are no tables.
                    let msg = Message::Error("No table found".to_string());
                    conn.send(&SignedMessage::new(&self.sk, msg)).await?;
                    bail!("No table found");
                }
            }
            msg => {
                if let Some(table) = &self.table {
                    table.handle_message(&player_id, msg);
                } else {
                    bail!("Invalid message {player_id} didn't join a table");
                }
            }
        }

        Ok(())
    }
}
