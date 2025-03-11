// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Network I/O.
use anyhow::{Result, anyhow};
use tokio::sync::{mpsc, oneshot};

use freezeout_core::{
    connection,
    crypto::{PeerId, SigningKey},
    message::{Message, SignedMessage},
};

/// A network event.
#[derive(Debug)]
enum Event {
    /// An incoming message.
    Message(SignedMessage),
    /// Connection has closed.
    ConnectionClosed,
    /// Connection error.
    Error(String),
}

/// A command for the network task.
#[derive(Debug)]
enum Command {
    /// Connects to a given host and port.
    Connect {
        /// The server hostname or address.
        host: String,
        /// The server port.
        port: u16,
        /// The command result.
        result: oneshot::Sender<Result<()>>,
    },
    /// Sends a message to the server.
    Send {
        /// The message payload.
        msg: Message,
        /// The command result.
        result: oneshot::Sender<Result<()>>,
    },
}

/// Network interface.
pub struct Network {
    commands_tx: mpsc::Sender<Command>,
    events_rx: mpsc::Receiver<Event>,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_complete_rx: mpsc::Receiver<()>,
    player_id: PeerId,
}

impl Network {
    /// Create a new network connection.
    pub fn new(sk: SigningKey) -> Self {
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (events_tx, events_rx) = mpsc::channel(64);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (_shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(64);

        let player_id = sk.verifying_key().peer_id();

        let mut task = NetworkTask {
            sk,
            commands_rx,
            events_tx,
            shutdown_rx,
            _shutdown_complete_tx,
        };

        tokio::spawn(async move {
            if let Err(err) = task.run().await {
                let s = format!("Network error {err}");
                let _ = task.events_tx.send(Event::Error(s)).await;
            }
        });

        Self {
            commands_tx,
            events_rx,
            shutdown_tx,
            shutdown_complete_rx,
            player_id,
        }
    }

    /// Returns the local player id.
    pub fn player_id(&self) -> PeerId {
        self.player_id.clone()
    }

    /// Wait for a message from the network.
    pub async fn recv(&mut self) -> Result<SignedMessage> {
        match self.events_rx.recv().await {
            Some(Event::Message(msg)) => Ok(msg),
            Some(Event::Error(e)) => Err(anyhow!("Network error: {e}")),
            Some(Event::ConnectionClosed) | None => Err(anyhow!("Connection closed")),
        }
    }

    /// Stops network service and disconnect.
    pub async fn shutdown(&mut self) {
        let _ = self.shutdown_tx.send(()).await;
        let _ = self.shutdown_complete_rx.recv().await;
    }

    /// Connect to the server.
    pub async fn connect(&self, host: &str, port: u16) -> Result<()> {
        let (res_tx, res_rx) = oneshot::channel();

        self.commands_tx
            .send(Command::Connect {
                host: host.to_string(),
                port,
                result: res_tx,
            })
            .await?;

        res_rx.await?
    }

    /// Sends a message to the client if connected.
    pub async fn send(&self, msg: Message) -> Result<()> {
        let (res_tx, res_rx) = oneshot::channel();

        self.commands_tx
            .send(Command::Send {
                msg,
                result: res_tx,
            })
            .await?;

        res_rx.await?
    }
}

struct NetworkTask {
    sk: SigningKey,
    commands_rx: mpsc::Receiver<Command>,
    events_tx: mpsc::Sender<Event>,
    shutdown_rx: mpsc::Receiver<()>,
    _shutdown_complete_tx: mpsc::Sender<()>,
}

impl NetworkTask {
    async fn run(&mut self) -> Result<()> {
        // Wait for connection command.
        let mut conn = loop {
            let cmd = tokio::select! {
                res = self.commands_rx.recv() => match res {
                    Some(cmd) => cmd,
                    None => return Ok(()),
                },
                _ = self.shutdown_rx.recv() => {
                    return Ok(());
                }
            };

            match cmd {
                Command::Connect { host, port, result } => {
                    let addr = format!("{host}:{port}");
                    match connection::connect_async(&addr).await {
                        Ok(conn) => {
                            let _ = result.send(Ok(()));
                            break conn;
                        }
                        Err(e) => {
                            let e = anyhow!("Connect error: {e}");
                            let _ = result.send(Err(e));
                        }
                    }
                }
                Command::Send { result, .. } => {
                    let _ = result.send(Err(anyhow!("Not connected")));
                }
            };
        };

        let res = loop {
            enum Branch {
                Conn(SignedMessage),
                Command(Command),
            }

            let branch = tokio::select! {
                // We have received a message from the client.
                res = conn.recv() => match res {
                    Some(Ok(msg)) =>  Branch::Conn(msg),
                    Some(Err(err)) => break Err(err),
                    None => break Ok(()),
                },
                // We have received a message from the table.
                res = self.commands_rx.recv() => match res {
                    Some(cmd) => Branch::Command(cmd),
                    None => break Ok(()),
                },
                // Server is shutting down exit this handler.
                _ = self.shutdown_rx.recv() => break Ok(()),
            };

            match branch {
                Branch::Conn(msg) => {
                    let _ = self.events_tx.send(Event::Message(msg)).await;
                }
                Branch::Command(cmd) => match cmd {
                    Command::Connect { result, .. } => {
                        let _ = result.send(Err(anyhow!("Already connected")));
                    }
                    Command::Send { msg, result } => {
                        let msg = SignedMessage::new(&self.sk, msg);
                        let res = conn.send(&msg).await;
                        let _ = result.send(res);
                    }
                },
            }
        };

        conn.close().await;
        let _ = self.events_tx.send(Event::ConnectionClosed).await;

        res
    }
}
