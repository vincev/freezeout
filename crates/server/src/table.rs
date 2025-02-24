// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table implementation.
use anyhow::Result;
use log::{error, info};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time,
};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::SignedMessage,
    poker::{Chips, TableId},
};

use crate::db::Db;

mod player;
mod state;

use state::State;

/// Table state shared by all players who joined the table.
#[derive(Debug)]
pub struct Table {
    /// Channel for sending commands.
    commands_tx: mpsc::Sender<TableCommand>,
    /// This table id.
    table_id: TableId,
}

/// A message sent to player connections.
#[derive(Debug)]
pub enum TableMessage {
    /// Sends a message to a client.
    Send(SignedMessage),
    /// Tell the client to leave the table.
    LeaveTable,
    /// The game has ended.
    EndGame,
    /// Close a client connection.
    Close,
}

/// Command for the table task.
#[derive(Debug)]
enum TableCommand {
    /// Join this table.
    Join {
        player_id: PeerId,
        nickname: String,
        join_chips: Chips,
        table_tx: mpsc::Sender<TableMessage>,
        resp_tx: oneshot::Sender<Result<()>>,
    },
    /// Query if the table game has started.
    HasGameStarted { resp_tx: oneshot::Sender<bool> },
    /// Query if all players left the table.
    IsEmpty { resp_tx: oneshot::Sender<bool> },
    /// Leave this table.
    Leave(PeerId),
    /// Handle a player message.
    Message(SignedMessage),
}

impl Table {
    /// Creates a new table that manages players and game state.
    pub fn new(
        seats: usize,
        sk: Arc<SigningKey>,
        db: Db,
        shutdown_broadcast_rx: broadcast::Receiver<()>,
        shutdown_complete_tx: mpsc::Sender<()>,
    ) -> Self {
        // There must be at least 2 seats.
        assert!(seats > 1);

        let (commands_tx, commands_rx) = mpsc::channel(128);

        let table_id = TableId::new_id();

        let mut task = TableTask {
            table_id,
            seats,
            sk,
            db,
            commands_rx,
            shutdown_broadcast_rx,
            _shutdown_complete_tx: shutdown_complete_tx,
        };

        tokio::spawn(async move {
            if let Err(err) = task.run().await {
                error!("Table {} error {err}", task.table_id);
            }

            info!("Table task for table {} stopped", task.table_id);
        });

        Self {
            commands_tx,
            table_id,
        }
    }

    /// Returns this table id.
    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    /// Checks if this table is waiting for players to join.
    pub async fn has_game_started(&self) -> bool {
        let (resp_tx, resp_rx) = oneshot::channel();

        let res = self
            .commands_tx
            .send(TableCommand::HasGameStarted { resp_tx })
            .await
            .is_ok();
        if !res {
            false
        } else {
            resp_rx.await.unwrap_or(false)
        }
    }

    /// Checks if this table is waiting for players to join.
    pub async fn is_empty(&self) -> bool {
        let (resp_tx, resp_rx) = oneshot::channel();

        let res = self
            .commands_tx
            .send(TableCommand::IsEmpty { resp_tx })
            .await
            .is_ok();
        if !res {
            false
        } else {
            resp_rx.await.unwrap_or(false)
        }
    }

    /// A player joins this table.
    ///
    /// Returns error if the table is full or the player has already joined.
    pub async fn join(
        &self,
        player_id: &PeerId,
        nickname: &str,
        join_chips: Chips,
        table_tx: mpsc::Sender<TableMessage>,
    ) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.commands_tx
            .send(TableCommand::Join {
                player_id: player_id.clone(),
                nickname: nickname.to_string(),
                join_chips,
                table_tx,
                resp_tx,
            })
            .await?;

        resp_rx.await?
    }

    /// A player leaves the table.
    pub async fn leave(&self, player_id: &PeerId) {
        let _ = self
            .commands_tx
            .send(TableCommand::Leave(player_id.clone()))
            .await;
    }

    /// Handle a message from a player.
    pub async fn message(&self, msg: SignedMessage) {
        let _ = self.commands_tx.send(TableCommand::Message(msg)).await;
    }
}

struct TableTask {
    /// This table identifie.
    table_id: TableId,
    /// Table seats.
    seats: usize,
    /// Table key.
    sk: Arc<SigningKey>,
    /// Game db.
    db: Db,
    /// Channel for receiving table commands.
    commands_rx: mpsc::Receiver<TableCommand>,
    /// Channel for listening shutdown notification.
    shutdown_broadcast_rx: broadcast::Receiver<()>,
    /// Sender that drops when this connection is done.
    _shutdown_complete_tx: mpsc::Sender<()>,
}

impl TableTask {
    async fn run(&mut self) -> Result<()> {
        let mut state = State::new(self.table_id, self.seats, self.sk.clone(), self.db.clone());
        let mut ticks = time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                // Server is shutting down exit this handler.
                _ = self.shutdown_broadcast_rx.recv() => break Ok(()),
                _ = ticks.tick() => {
                    state.tick().await;
                }
                // We have received a message from the client.
                res = self.commands_rx.recv() => match res {
                    Some(TableCommand::Join{ player_id, nickname, join_chips, table_tx, resp_tx }) => {
                        let res = state.join(&player_id, &nickname, join_chips, table_tx).await;
                        let _ = resp_tx.send(res);
                    }
                    Some(TableCommand::HasGameStarted { resp_tx }) => {
                        let res = state.has_game_started();
                        let _ = resp_tx.send(res);
                    }
                    Some(TableCommand::IsEmpty { resp_tx }) => {
                        let res = state.is_empty();
                        let _ = resp_tx.send(res);
                    }
                    Some(TableCommand::Leave(peer_id)) => {
                        state.leave(&peer_id).await;
                    }
                    Some(TableCommand::Message(msg)) => {
                        state.message(msg).await;

                    }
                    None => break Ok(()),
                },
            }
        }
    }
}
