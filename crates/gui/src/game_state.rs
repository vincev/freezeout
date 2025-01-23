// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Game view.
use log::info;

use freezeout_core::{
    crypto::PeerId,
    message::{Message, PlayerAction, PlayerUpdate, SignedMessage},
    poker::{Card, Chips, PlayerCards, TableId},
};

use crate::App;

/// Game player data.
#[derive(Debug)]
pub struct Player {
    /// This player id.
    pub player_id: PeerId,
    /// Cache player id digits to avoid generation at every repaint.
    pub player_id_digits: String,
    /// This player nickname.
    pub nickname: String,
    /// This player chips.
    pub chips: Chips,
    /// The last player bet.
    pub bet: Chips,
    /// The last player action.
    pub action: PlayerAction,
    /// The last player action.
    pub action_timer: Option<u16>,
    /// This playe cards.
    pub cards: PlayerCards,
    /// The player has the button.
    pub has_button: bool,
    /// The player is active in the hand.
    pub is_active: bool,
}

impl Player {
    fn new(player_id: PeerId, nickname: String, chips: Chips) -> Self {
        Self {
            player_id_digits: player_id.digits(),
            player_id,
            nickname,
            chips,
            bet: Chips::ZERO,
            action: PlayerAction::None,
            action_timer: None,
            cards: PlayerCards::None,
            has_button: false,
            is_active: true,
        }
    }
}

/// A player action request from the server.
#[derive(Debug)]
pub struct ActionRequest {
    /// The actions choices requested by server.
    pub actions: Vec<PlayerAction>,
    /// The action minimum raise
    pub min_raise: Chips,
    /// The hand big blind.
    pub big_blind: Chips,
}

/// This client game state.
#[derive(Debug)]
pub struct GameState {
    table_id: TableId,
    error: Option<String>,
    players: Vec<Player>,
    action_request: Option<ActionRequest>,
    board: Vec<Card>,
    pot: Chips,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            table_id: TableId::NO_TABLE,
            error: None,
            players: Vec::default(),
            action_request: None,
            board: Vec::default(),
            pot: Chips::ZERO,
        }
    }
}

impl GameState {
    /// Handle an incoming server message.
    pub fn handle_message(&mut self, msg: SignedMessage, app: &mut App) {
        match msg.message() {
            Message::TableJoined { table_id, chips } => {
                self.table_id = *table_id;
                // Add this player as the first player in the players list.
                self.players.push(Player::new(
                    app.player_id().clone(),
                    app.nickname().to_string(),
                    *chips,
                ));

                info!(
                    "Joined table {} {:?}",
                    table_id,
                    self.players.last().unwrap()
                )
            }
            Message::PlayerJoined {
                player_id,
                nickname,
                chips,
            } => {
                self.players
                    .push(Player::new(player_id.clone(), nickname.clone(), *chips));

                info!("Added player {:?}", self.players.last().unwrap())
            }
            Message::PlayerLeft(player_id) => {
                self.players.retain(|p| &p.player_id != player_id);
            }
            Message::StartGame(seats) => {
                // Reorder seats according to the new order.
                for (idx, seat_id) in seats.iter().enumerate() {
                    let pos = self
                        .players
                        .iter()
                        .position(|p| &p.player_id == seat_id)
                        .expect("Player not found");
                    self.players.swap(idx, pos);
                }

                // Move local player in first position.
                let pos = self
                    .players
                    .iter()
                    .position(|p| &p.player_id == app.player_id())
                    .expect("Local player not found");
                self.players.rotate_left(pos);
            }
            Message::StartHand => {
                // Prepare for a new hand.
                for player in &mut self.players {
                    player.cards = PlayerCards::None;
                    player.action = PlayerAction::None;
                }
            }
            Message::DealCards(c1, c2) => {
                // This client player should be in first position.
                assert!(!self.players.is_empty());
                assert!(&self.players[0].player_id == app.player_id());

                self.players[0].cards = PlayerCards::Cards(*c1, *c2);
                info!(
                    "Player {} received cards {:?}",
                    app.player_id(),
                    self.players[0].cards
                );
            }
            Message::GameUpdate {
                players,
                board,
                pot,
            } => {
                self.update_players(players);
                self.board = board.clone();
                self.pot = *pot;
            }
            Message::Error(e) => self.error = Some(e.clone()),
            Message::ActionRequest {
                player_id,
                min_raise,
                big_blind,
                actions,
            } => {
                // Check if the action has been requested for this player.
                if app.player_id() == player_id {
                    info!(
                        "Player {} request action: {} {:?}",
                        player_id, min_raise, actions
                    );
                    self.action_request = Some(ActionRequest {
                        actions: actions.clone(),
                        min_raise: *min_raise,
                        big_blind: *big_blind,
                    });
                }
            }
            _ => {}
        }
    }

    /// Returns the requested player action if any.
    pub fn take_action_request(&mut self) -> Option<ActionRequest> {
        self.action_request.take()
    }

    /// Returns a reference to the players.
    pub fn players(&self) -> &[Player] {
        &self.players
    }

    /// The current pot.
    pub fn pot(&self) -> Chips {
        self.pot
    }

    /// The board cards.
    pub fn board(&self) -> &[Card] {
        &self.board
    }

    /// Checks if the local player is active.
    pub fn is_active(&self) -> bool {
        !self.players.is_empty() && self.players[0].is_active
    }

    fn update_players(&mut self, updates: &[PlayerUpdate]) {
        for update in updates {
            if let Some(pos) = self
                .players
                .iter_mut()
                .position(|p| p.player_id == update.player_id)
            {
                let player = &mut self.players[pos];
                player.chips = update.chips;
                player.bet = update.bet;
                player.action = update.action;
                player.action_timer = update.action_timer;
                player.has_button = update.has_button;
                player.is_active = update.is_active;

                // Do not override cards for the local player as they are updated
                // when we get a DealCards message.
                if pos != 0 {
                    player.cards = update.cards;
                }

                // If local player has folded remove its cards.
                if pos == 0 && !player.is_active {
                    player.cards = PlayerCards::None;
                }
            }
        }
    }
}
