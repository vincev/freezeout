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
    table::{Table, TableMessage},
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

    let sk = Arc::new(SigningKey::default());

    let mut server = Server {
        tables: Arc::new(TablesSet::new(config.tables, config.seats, sk.clone())),
        sk,
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
                sk: self.sk.clone(),
                tables: self.tables.clone(),
                shutdown_broadcast_rx: self.shutdown_broadcast_tx.subscribe(),
                _shutdown_complete_tx: self.shutdown_complete_tx.clone(),
            };

            // Spawn a task to handle connection messages.
            tokio::spawn(async move {
                if let Err(err) = handler.run(socket).await {
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
    fn new(tables: usize, seats: usize, sk: Arc<SigningKey>) -> Self {
        Self(
            (0..tables)
                .map(|_| Arc::new(Table::new(seats, sk.clone())))
                .collect(),
        )
    }

    /// Join a table on this set.
    async fn join_table(
        &self,
        player_id: &PlayerId,
        nickname: &str,
    ) -> Option<(Arc<Table>, mpsc::Receiver<TableMessage>)> {
        for table in &self.0 {
            if let Ok(table_rx) = table.join(player_id, nickname).await {
                return Some((table.clone(), table_rx));
            }
        }

        None
    }
}

impl Handler {
    /// Handle connection messages.
    async fn run(&mut self, socket: TcpStream) -> Result<()> {
        let mut conn = connection::accept_async(socket).await?;
        let res = self.handle_connection(&mut conn).await;
        conn.close().await;
        res
    }

    /// Handle connection messages.
    async fn handle_connection(&mut self, conn: &mut EncryptedConnection) -> Result<()> {
        // Wait for the first client message to get player id and join a table.
        let msg = tokio::select! {
            _ = self.shutdown_broadcast_rx.recv() => {
                return Ok(());
            }
            res = conn.recv() => match res {
                Some(Ok(msg)) => msg,
                Some(Err(err)) => return Err(err),
                None => return Ok(()),
            },
        };

        // Try to join a table and get a table message channel.
        let player_id = msg.player_id();
        let (table, mut table_rx) = match msg.to_message() {
            Message::JoinTable(nickname) => {
                if let Some((table, table_rx)) = self.tables.join_table(&player_id, &nickname).await
                {
                    (table, table_rx)
                } else {
                    // Notify the client that there are no tables.
                    let msg = Message::Error("No table found".to_string());
                    conn.send(&SignedMessage::new(&self.sk, msg)).await?;
                    bail!("No table found");
                }
            }
            _ => bail!("Invalid message {player_id} didn't join a table"),
        };

        let res = loop {
            tokio::select! {
                // Server is shutting down exit this handler.
                _ = self.shutdown_broadcast_rx.recv() => break Ok(()),
                // We have received a message from the client.
                res = conn.recv() => match res {
                    Some(Ok(msg)) => table.handle_message(msg).await,
                    Some(Err(err)) => break Err(err),
                    None => break Ok(()),
                },
                // We have received a message from the table.
                res = table_rx.recv() => match res {
                    Some(TableMessage::Send(msg)) => {
                        let res = conn.send(&msg).await;
                        if res.is_err() {
                            break res;
                        }
                    }
                    Some(TableMessage::Close) => {
                        info!("Connection closed by table message");
                        break Ok(());
                    },
                    None => {},
                }
            }
        };

        table.leave(&player_id).await;

        res
    }
}
