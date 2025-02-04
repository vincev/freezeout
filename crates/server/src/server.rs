// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker server entry point.
use anyhow::{anyhow, bail, Result};
use log::{error, info};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    net::{TcpListener, TcpStream},
    signal,
    sync::{broadcast, mpsc},
    time::{self, Duration},
};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::{Message, SignedMessage},
    poker::Chips,
};

use crate::{
    connection::{self, EncryptedConnection},
    db::Db,
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
    /// Server identity keypair path.
    pub keypair_path: Option<PathBuf>,
    /// Game database path.
    pub db_path: Option<PathBuf>,
}

/// Server entry point.
pub async fn run(config: Config) -> Result<()> {
    let sk = load_signing_key(&config.keypair_path)?;
    let db = open_database(&config.db_path)?;

    let addr = format!("{}:{}", config.address, config.port);
    info!("Starting server listening on {}", addr);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow!("Tcp listener bind error: {e}"))?;

    let shutdown_signal = signal::ctrl_c();
    let (shutdown_broadcast_tx, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let tables = Arc::new(Tables::new(
        config.tables,
        config.seats,
        sk.clone(),
        db.clone(),
        &shutdown_broadcast_tx,
        &shutdown_complete_tx,
    ));

    let mut server = Server {
        tables,
        sk,
        db,
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

/// The server that handles client connection and state.
#[derive(Debug)]
struct Server {
    /// The tables on this server.
    tables: Arc<Tables>,
    /// The server signing key shared by all connections.
    sk: Arc<SigningKey>,
    /// The players DB.
    db: Db,
    /// The server listener.
    listener: TcpListener,
    /// Shutdown notification channel.
    shutdown_broadcast_tx: broadcast::Sender<()>,
    /// Shutdown sender cloned by each connection.
    shutdown_complete_tx: mpsc::Sender<()>,
}

impl Server {
    /// Runs the server.
    async fn run(&mut self) -> Result<()> {
        loop {
            let (socket, addr) = self.accept_with_retry().await?;
            info!("Accepted connection from {addr}");

            let mut handler = Handler {
                tables: self.tables.clone(),
                sk: self.sk.clone(),
                db: self.db.clone(),
                table: None,
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

/// The tables on this server.
#[derive(Debug)]
struct Tables(Vec<Arc<Table>>);

impl Tables {
    /// Creates a new table set.
    fn new(
        tables: usize,
        seats: usize,
        sk: Arc<SigningKey>,
        db: Db,
        shutdown_broadcast_tx: &broadcast::Sender<()>,
        shutdown_complete_tx: &mpsc::Sender<()>,
    ) -> Self {
        Self(
            (0..tables)
                .map(|_| {
                    Arc::new(Table::new(
                        seats,
                        sk.clone(),
                        db.clone(),
                        shutdown_broadcast_tx.subscribe(),
                        shutdown_complete_tx.clone(),
                    ))
                })
                .collect(),
        )
    }

    async fn join(
        &self,
        player_id: &PeerId,
        nickname: &str,
        join_chips: Chips,
        table_tx: mpsc::Sender<TableMessage>,
    ) -> Option<Arc<Table>> {
        for table in &self.0 {
            if table
                .join(player_id, nickname, join_chips, table_tx.clone())
                .await
                .is_ok()
            {
                return Some(table.clone());
            }
        }

        None
    }
}

/// Client connection handler.
struct Handler {
    /// The tables on this server.
    tables: Arc<Tables>,
    /// The server signing key shared by all connections.
    sk: Arc<SigningKey>,
    /// The players DB.
    db: Db,
    /// This client table.
    table: Option<Arc<Table>>,
    /// Channel for listening shutdown notification.
    shutdown_broadcast_rx: broadcast::Receiver<()>,
    /// Sender that drops when this connection is done.
    _shutdown_complete_tx: mpsc::Sender<()>,
}

impl Handler {
    const JOIN_SERVER_CHIPS: Chips = Chips::new(3_000_000);
    const JOIN_TABLE_CHIPS: Chips = Chips::new(1_000_000);

    /// Handle connection messages.
    async fn run(&mut self, socket: TcpStream) -> Result<()> {
        let mut conn = connection::accept_async(socket).await?;
        let res = self.handle_connection(&mut conn).await;
        conn.close().await;
        res
    }

    /// Handle connection messages.
    async fn handle_connection(&mut self, conn: &mut EncryptedConnection) -> Result<()> {
        // Wait for a JoinServer message from the client to join this server and get
        // the client nickname and player id.
        let msg = tokio::select! {
            res = conn.recv() => match res {
                Some(Ok(msg)) =>  msg,
                Some(Err(err)) => return Err(err),
                None => return Ok(()),
            },
            _ = self.shutdown_broadcast_rx.recv() => {
                return Ok(());
            }
        };

        let (nickname, player_id) = match msg.message() {
            Message::JoinServer { nickname } => {
                let player = self
                    .db
                    .join_server(msg.sender(), nickname, Self::JOIN_SERVER_CHIPS)
                    .await?;

                // Notify client with the player account.
                let smsg = SignedMessage::new(
                    &self.sk,
                    Message::ServerJoined {
                        nickname: player.nickname,
                        chips: player.chips,
                    },
                );

                conn.send(&smsg).await?;

                (nickname.to_string(), msg.sender())
            }
            _ => bail!(
                "Invalid message from {} expecting a join server.",
                msg.sender()
            ),
        };

        // Create channel to get messages from a table.
        let (table_tx, mut table_rx) = mpsc::channel(128);

        let res = loop {
            enum Branch {
                Conn(SignedMessage),
                Table(TableMessage),
            }

            let branch = tokio::select! {
                // We have received a message from the client.
                res = conn.recv() => match res {
                    Some(Ok(msg)) =>  Branch::Conn(msg),
                    Some(Err(err)) => break Err(err),
                    None => break Ok(()),
                },
                // We have received a message from the table.
                res = table_rx.recv() => match res {
                    Some(msg) => Branch::Table(msg),
                    None => break Ok(()),
                },
                // Server is shutting down exit this handler.
                _ = self.shutdown_broadcast_rx.recv() => break Ok(()),
            };

            match branch {
                Branch::Conn(msg) => match msg.message() {
                    Message::JoinTable => {
                        let join_chips = Self::JOIN_TABLE_CHIPS;
                        // Pay chips to joins a table.
                        let has_chips = self
                            .db
                            .pay_from_player(player_id.clone(), join_chips)
                            .await?;
                        if has_chips {
                            // Try to find a table
                            self.table = self
                                .tables
                                .join(&player_id, &nickname, join_chips, table_tx.clone())
                                .await;
                            // If no table has been found refund chips and notify client.
                            if self.table.is_none() {
                                self.db.pay_to_player(player_id.clone(), join_chips).await?;

                                conn.send(&SignedMessage::new(&self.sk, Message::NoTablesLeft))
                                    .await?;
                            }
                        } else {
                            // If this player doesn't have enough chips to join a
                            // table notify the client.
                            conn.send(&SignedMessage::new(&self.sk, Message::NotEnoughChips))
                                .await?;
                        }
                    }
                    _ => {
                        if let Some(table) = &self.table {
                            table.message(msg).await;
                        }
                    }
                },
                Branch::Table(msg) => match msg {
                    TableMessage::Send(msg) => {
                        if let err @ Err(_) = conn.send(&msg).await {
                            break err;
                        }
                    }
                    TableMessage::PlayerLeft => {
                        // If a player leaves the table reset the table and send
                        // updated player account information to the client.
                        self.table = None;

                        let player = self.db.get_player(player_id.clone()).await?;

                        // Tell the client to show the account dialog.
                        let msg = Message::ShowAccount {
                            chips: player.chips,
                        };

                        conn.send(&SignedMessage::new(&self.sk, msg)).await?;
                    }
                    TableMessage::Close => {
                        info!("Connection closed by table message");
                        break Ok(());
                    }
                },
            }
        };

        if let Some(table) = &self.table {
            table.leave(&player_id).await;
        }

        res
    }
}

fn load_signing_key(path: &Option<PathBuf>) -> Result<Arc<SigningKey>> {
    fn load_or_create(path: &Path) -> Result<Arc<SigningKey>> {
        let keypair_path = path.join("server.phrase");
        let keypair = if keypair_path.exists() {
            info!("Loading keypair {}", keypair_path.display());
            let passphrase = std::fs::read_to_string(keypair_path)?;
            SigningKey::from_phrase(&passphrase)?
        } else {
            let keypair = SigningKey::default();
            std::fs::create_dir_all(path)?;
            std::fs::write(&keypair_path, keypair.phrase().as_bytes())?;
            info!("Writing keypair {}", keypair_path.display());
            keypair
        };

        Ok(Arc::new(keypair))
    }

    // Load keypair from user path or try to create one if it doesn't exist.
    if let Some(path) = path {
        load_or_create(path)
    } else {
        let Some(proj_dirs) = directories::ProjectDirs::from("", "", "freezeout") else {
            bail!("Cannot find project dirs");
        };

        load_or_create(proj_dirs.config_dir())
    }
}

fn open_database(path: &Option<PathBuf>) -> Result<Db> {
    fn load_or_create(path: &Path) -> Result<Db> {
        let db_path = path.join("game.db");
        if db_path.exists() {
            info!("Loading database {}", db_path.display());
            Db::open(db_path)
        } else {
            std::fs::create_dir_all(path)?;
            info!("Writing database {}", db_path.display());
            Db::open(db_path)
        }
    }

    // Load database from user path or try to create one if it doesn't exist.
    if let Some(path) = path {
        load_or_create(path)
    } else {
        let Some(proj_dirs) = directories::ProjectDirs::from("", "", "freezeout") else {
            bail!("Cannot find project dirs");
        };

        load_or_create(proj_dirs.config_dir())
    }
}
