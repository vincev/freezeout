// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Automated poker client.
use anyhow::Result;
use rand::prelude::*;
use tokio::{
    sync::{broadcast, mpsc},
    time::{self, Duration},
};

use freezeout_core::{
    connection,
    crypto::SigningKey,
    game_state::{ActionRequest, GameState},
    message::{Message, PlayerAction, SignedMessage},
    poker::Chips,
};

/// A Poker bot strategy.
pub trait Strategy: Send + 'static {
    /// Execute an action given a game state.
    fn execute(&mut self, req: &ActionRequest, state: &GameState) -> (PlayerAction, Chips);
}

/// Poker client.
pub struct Client<S: Strategy> {
    strategy: S,
    nickname: String,
    conn: connection::EncryptedConnection,
    sk: SigningKey,
    shutdown_broadcast_rx: broadcast::Receiver<()>,
    _shutdown_complete_tx: mpsc::Sender<()>,
}

impl<S: Strategy> Client<S> {
    /// Creates a new client.
    pub async fn new(
        strategy: S,
        nickname: String,
        host: &str,
        port: u16,
        shutdown_broadcast_rx: broadcast::Receiver<()>,
        _shutdown_complete_tx: mpsc::Sender<()>,
    ) -> Result<Self> {
        // Try to connect and join the server.
        let addr = format!("{host}:{port}");
        let mut conn = connection::connect_async(&addr).await?;

        let sk = SigningKey::default();
        let msg = SignedMessage::new(
            &sk,
            Message::JoinServer {
                nickname: nickname.clone(),
            },
        );

        // Request to join server with the given nickname.
        conn.send(&msg).await?;

        Ok(Self {
            strategy,
            nickname,
            sk,
            conn,
            shutdown_broadcast_rx,
            _shutdown_complete_tx,
        })
    }

    /// Runs the client message loop.
    pub async fn run(&mut self) -> Result<()> {
        let mut state = GameState::new(self.sk.verifying_key().peer_id(), self.nickname.clone());

        loop {
            let msg = tokio::select! {
                res = self.conn.recv() => match res {
                    Some(Ok(msg)) =>  msg,
                    Some(Err(err)) => return Err(err),
                    None => return Ok(()),
                },
                _ = self.shutdown_broadcast_rx.recv() => {
                    self.conn.close().await;
                    return Ok(());
                }
            };

            // If this is a server joined confirmation try to join a table.
            if let Message::ServerJoined { .. } = msg.message() {
                self.send(Message::JoinTable).await?;
            } else {
                state.handle_message(msg);

                if let Some(req) = state.action_request() {
                    let delay = thread_rng().gen_range(500..3000);
                    time::sleep(Duration::from_millis(delay)).await;

                    let (action, amount) = self.strategy.execute(req, &state);

                    self.send(Message::ActionResponse { action, amount })
                        .await?;

                    state.reset_action_request();
                }
            }
        }
    }

    async fn send(&mut self, msg: Message) -> Result<()> {
        let msg = SignedMessage::new(&self.sk, msg);
        self.conn.send(&msg).await
    }
}
