// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table state types.
use anyhow::{bail, Result};
use log::{error, info};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::{Message, PlayerAction, PlayerUpdate, SignedMessage},
    poker::{Chips, Deck, PlayerCards, TableId},
};

/// Table state shared by all players who joined the table.
#[derive(Debug)]
pub struct Table {
    /// Channel for sending commands.
    commands_tx: mpsc::Sender<TableCommand>,
}

/// A message sent to player connections.
#[derive(Debug)]
pub enum TableMessage {
    /// Sends a message to a client.
    Send(SignedMessage),
    /// Close a client connection.
    Close,
}

/// Command for the table task.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum TableCommand {
    /// Join this table.
    Join(
        PeerId,
        String,
        oneshot::Sender<Result<mpsc::Receiver<TableMessage>>>,
    ),
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
        shutdown_broadcast_rx: broadcast::Receiver<()>,
        shutdown_complete_tx: mpsc::Sender<()>,
    ) -> Self {
        // There must be at least 2 seats.
        assert!(seats > 1);

        let (commands_tx, commands_rx) = mpsc::channel(128);

        let mut task = TableTask {
            table_id: TableId::new_id(),
            seats,
            sk,
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

        Self { commands_tx }
    }

    /// A player joins this table.
    ///
    /// Returns error if the table is full or the player has already joined.
    pub async fn join(
        &self,
        player_id: &PeerId,
        nickname: &str,
    ) -> Result<mpsc::Receiver<TableMessage>> {
        let (res_tx, res_rx) = oneshot::channel();

        self.commands_tx
            .send(TableCommand::Join(
                player_id.clone(),
                nickname.to_string(),
                res_tx,
            ))
            .await?;

        res_rx.await?
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
    /// Channel for receiving table commands.
    commands_rx: mpsc::Receiver<TableCommand>,
    /// Channel for listening shutdown notification.
    shutdown_broadcast_rx: broadcast::Receiver<()>,
    /// Sender that drops when this connection is done.
    _shutdown_complete_tx: mpsc::Sender<()>,
}

impl TableTask {
    async fn run(&mut self) -> Result<()> {
        let mut state = State::new(self.table_id, self.seats, self.sk.clone());
        loop {
            tokio::select! {
                // Server is shutting down exit this handler.
                _ = self.shutdown_broadcast_rx.recv() => break Ok(()),
                // We have received a message from the client.
                res = self.commands_rx.recv() => match res {
                    Some(TableCommand::Join(peer_id, nickname, res_tx)) => {
                        let res = state.join(&peer_id, &nickname).await;
                        let _ = res_tx.send(res);
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

/// The hand state.
#[derive(Debug)]
enum HandState {
    /// The table is waiting for players to join before starting the game.
    WaitForPlayers,
    /// Start the hand, collect blinds and deal cards.
    StartHand,
    /// Handle preflop betting.
    PreflopBetting,
    /// Deal flop cards.
    DealFlop,
    /// Handle flop betting.
    FlopBetting,
    /// Deal turn card.
    DealTurn,
    /// Handle turn betting.
    TurnBetting,
    /// Deal river card.
    DealRiver,
    /// Handle river players action.
    RiverBetting,
    /// Showdown.
    Showdown,
    /// The hand has ended.
    EndHand,
    /// The game has ended with a winner.
    EndGame,
}

/// A table player state.
#[derive(Debug)]
struct Player {
    /// The player peer id.
    player_id: PeerId,
    /// The channel to send messages to this player connection.
    table_tx: mpsc::Sender<TableMessage>,
    /// This playe nickname.
    nickname: String,
    /// This player chips.
    chips: Chips,
    /// The player bet amount.
    bet: Chips,
    /// The last player action.
    action: PlayerAction,
    /// This player cards that are visible to all other players.
    public_cards: PlayerCards,
    /// This player private cards.
    hole_cards: PlayerCards,
    /// This player is active in the hand.
    is_active: bool,
}

impl Player {
    /// Send a message to this player connection.
    async fn send(&self, msg: SignedMessage) {
        let _ = self.table_tx.send(TableMessage::Send(msg)).await;
    }

    /// Reset state for a new hand.
    fn start_hand(&mut self) {
        self.is_active = self.chips > Chips::ZERO;
        self.bet = Chips::ZERO;
        self.action = PlayerAction::None;
        self.public_cards = PlayerCards::None;
        self.hole_cards = PlayerCards::None;
    }

    /// Updates this player bets to the given chips amount.
    fn bet(&mut self, action: PlayerAction, chips: Chips) {
        // How much to bet considering previous bets.
        let remainder = chips - self.bet;

        // Player run out of chips goes all in.
        if self.chips < remainder {
            self.bet += self.chips;
            self.chips = Chips::ZERO;
        } else {
            self.bet += remainder;
            self.chips -= remainder;
        }

        self.action = action;
    }
}

/// Internal table state.
#[derive(Debug)]
struct State {
    table_id: TableId,
    seats: usize,
    join_chips: Chips,
    sk: Arc<SigningKey>,
    hand_state: HandState,
    small_blind: Chips,
    big_blind: Chips,
    players: Vec<Player>,
    deck: Deck,
    active_player: usize,
    last_bet: Chips,
    min_raise: Chips,
}

impl State {
    /// Create a new state.
    fn new(table_id: TableId, seats: usize, sk: Arc<SigningKey>) -> Self {
        Self {
            table_id,
            seats,
            join_chips: Chips(1_000_000),
            sk,
            hand_state: HandState::WaitForPlayers,
            small_blind: Chips(10_000),
            big_blind: Chips(20_000),
            players: Vec::with_capacity(seats),
            deck: Deck::new_and_shuffled(),
            active_player: 0,
            last_bet: Chips::ZERO,
            min_raise: Chips::ZERO,
        }
    }

    /// A player tries to join the table.
    async fn join(
        &mut self,
        player_id: &PeerId,
        nickname: &str,
    ) -> Result<mpsc::Receiver<TableMessage>> {
        if !matches!(self.hand_state, HandState::WaitForPlayers) {
            bail!("Hand in progress");
        }

        if self.players.len() == self.seats {
            bail!("Table full");
        }

        if self.players.iter().any(|p| &p.player_id == player_id) {
            bail!("Player has already joined");
        }

        // Tell all players at the table that a player joined.
        let msg = Message::PlayerJoined {
            player_id: player_id.clone(),
            nickname: nickname.to_string(),
            chips: self.join_chips,
        };
        self.broadcast(msg).await;

        let (table_tx, table_rx) = mpsc::channel(128);

        // Send a table joined confirmation to the player who joined.
        let msg = Message::TableJoined {
            table_id: self.table_id,
            chips: self.join_chips,
        };
        let smsg = SignedMessage::new(&self.sk, msg);
        let _ = table_tx.send(TableMessage::Send(smsg)).await;

        // Send joined message for each player at the table to the new player.
        for player in &self.players {
            let msg = Message::PlayerJoined {
                player_id: player.player_id.clone(),
                nickname: player.nickname.clone(),
                chips: player.chips,
            };
            let smsg = SignedMessage::new(&self.sk, msg);
            let _ = table_tx.send(TableMessage::Send(smsg)).await;
        }

        // Add new player to the table.
        let player = Player {
            player_id: player_id.clone(),
            table_tx,
            nickname: nickname.to_string(),
            chips: self.join_chips,
            bet: Chips::default(),
            action: PlayerAction::None,
            public_cards: PlayerCards::None,
            hole_cards: PlayerCards::None,
            is_active: true,
        };

        self.players.push(player);

        info!("Player {player_id} joined table {}", self.table_id);

        if self.players.len() == self.seats {
            self.enter_start_hand().await;
        }

        Ok(table_rx)
    }

    /// A player leaves the table.
    async fn leave(&mut self, player_id: &PeerId) {
        if let Some(pos) = self
            .players
            .iter_mut()
            .position(|p| &p.player_id == player_id)
        {
            self.players.remove(pos);

            let msg = Message::PlayerLeft(player_id.clone());
            self.broadcast(msg).await;

            if self.players.is_empty() {
                self.hand_state = HandState::WaitForPlayers;
            } else if self.players.len() == 1 {
                // If one player left the hand ends and the player wins the game.
                self.active_player = 0;
                self.enter_end_game().await;
            } else if pos < self.active_player {
                // Adjust active player if the player leaving acts before.
                self.active_player -= 1;
            } else if pos == self.active_player {
                // If the active player was the last activate first player otherwise
                // activate the player that moved in the same position.
                if pos == self.players.len() {
                    self.active_player = 0;
                }

                self.request_action().await;
            }

            // Nothing to do if the player leaving comes after the active player.
        }
    }

    /// Handle a message from a player.
    async fn message(&mut self, msg: SignedMessage) {
        info!("Player message: {msg:?}");
    }

    /// Start a new hand.
    async fn enter_start_hand(&mut self) {
        // Activate all players who have chips.
        for player in &mut self.players {
            player.start_hand();
        }

        // If there are fewer than 2 active players end the game.
        if self.count_active() < 2 {
            self.enter_end_game().await;
            return;
        }

        self.hand_state = HandState::StartHand;

        // Rotate players so that the first player becomes the button.
        loop {
            self.players.rotate_left(1);
            if self.players[0].is_active {
                // Checked above there are at least 2 active players.
                break;
            }
        }

        // Reset the active player to the fist player.
        self.active_player = 0;

        // Pay small and big blind.
        self.players[self.active_player].bet(PlayerAction::SmallBlind, self.small_blind);

        self.next_player();
        self.players[self.active_player].bet(PlayerAction::BigBlind, self.big_blind);

        self.last_bet = self.big_blind;
        self.min_raise = self.big_blind;

        // Create a new deck.
        self.deck = Deck::new_and_shuffled();

        // Tell clients to prepare for a new hand.
        self.broadcast(Message::StartHand).await;

        // Deal cards to each player.
        for player in &mut self.players {
            if player.is_active {
                player.public_cards = PlayerCards::Covered;
                player.hole_cards = PlayerCards::Cards(self.deck.deal(), self.deck.deal());
            } else {
                player.public_cards = PlayerCards::None;
                player.hole_cards = PlayerCards::None;
            }
        }

        // Tell clients to update all players state.
        self.broadcast_game_update().await;

        // Deal the cards to each player.
        for player in &self.players {
            if let PlayerCards::Cards(c1, c2) = player.hole_cards {
                let msg = Message::DealCards(c1, c2);
                let smsg = SignedMessage::new(&self.sk, msg);
                player.send(smsg).await;
            }
        }

        // Activate next player and request action.
        self.next_player();
        self.request_action().await;
    }

    async fn enter_end_game(&mut self) {
        self.hand_state = HandState::EndGame;
        // TODO: End game logic.
    }

    /// Broadcast a game state update to all connected players.
    async fn broadcast_game_update(&self) {
        let players = self
            .players
            .iter()
            .map(|p| PlayerUpdate {
                player_id: p.player_id.clone(),
                chips: p.chips,
                bet: p.bet,
                action: p.action,
                cards: p.public_cards,
            })
            .collect();

        let msg = Message::GameUpdate { players };
        let smsg = SignedMessage::new(&self.sk, msg);
        for player in &self.players {
            player.send(smsg.clone()).await;
        }
    }

    /// Request action to the active player.
    async fn request_action(&self) {
        if self.count_active() > 1 {
            let player = &self.players[self.active_player];
            let mut actions = vec![PlayerAction::Fold];

            if self.last_bet == Chips::ZERO {
                actions.push(PlayerAction::Bet);
            }

            if player.bet == self.last_bet {
                actions.push(PlayerAction::Check);
            }

            if player.bet < self.last_bet {
                actions.push(PlayerAction::Call);
            }

            if player.chips >= self.last_bet && self.last_bet > Chips::ZERO {
                actions.push(PlayerAction::Raise);
            }

            let msg = Message::RequestAction {
                player_id: player.player_id.clone(),
                min_raise: self.min_raise,
                actions,
            };

            self.broadcast(msg).await;
        }
    }

    /// Broadcast a message to all players at the table.
    async fn broadcast(&self, msg: Message) {
        let smsg = SignedMessage::new(&self.sk, msg);
        for player in &self.players {
            player.send(smsg.clone()).await;
        }
    }

    fn count_active(&self) -> usize {
        self.players.iter().filter(|p| p.is_active).count()
    }

    fn next_player(&mut self) {
        if self.count_active() > 1 {
            loop {
                self.active_player = (self.active_player + 1) % self.players.len();
                if self.players[self.active_player].is_active {
                    break;
                }
            }
        }
    }
}
