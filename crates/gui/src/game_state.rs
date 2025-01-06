// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Game view.
use log::info;

use freezeout_core::{
    crypto::PeerId,
    message::{Message, PlayerAction, PlayerUpdate, SignedMessage},
    poker::{Chips, PlayerCards, TableId},
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
    /// This playe cards.
    pub cards: PlayerCards,
}

/// This client game state.
#[derive(Debug)]
pub struct GameState {
    table_id: TableId,
    error: Option<String>,
    players: Vec<Player>,
    actions_req: Vec<PlayerAction>,
    min_raise: Chips,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            table_id: TableId::NO_TABLE,
            error: None,
            players: Vec::default(),
            actions_req: Vec::default(),
            min_raise: Chips::ZERO,
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
                let player_id = app.player_id().clone();
                self.players.push(Player {
                    player_id_digits: player_id.digits(),
                    player_id,
                    nickname: app.nickname().to_string(),
                    chips: *chips,
                    bet: Chips::ZERO,
                    action: PlayerAction::None,
                    cards: PlayerCards::None,
                });

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
                self.players.push(Player {
                    player_id_digits: player_id.digits(),
                    player_id: player_id.clone(),
                    nickname: nickname.clone(),
                    chips: *chips,
                    bet: Chips::ZERO,
                    action: PlayerAction::None,
                    cards: PlayerCards::None,
                });

                info!("Added player {:?}", self.players.last().unwrap())
            }
            Message::PlayerLeft(player_id) => {
                self.players.retain(|p| &p.player_id != player_id);
            }
            Message::StartHand => {
                // Prepare for a new hand.
                for player in &mut self.players {
                    player.cards = PlayerCards::None;
                    player.action = PlayerAction::None;
                }

                self.actions_req.clear();
                self.min_raise = Chips::ZERO;
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
            Message::GameUpdate { players } => {
                self.update_players(players);
            }
            Message::Error(e) => self.error = Some(e.clone()),
            Message::RequestAction {
                player_id,
                min_raise,
                actions,
            } => {
                // Check if the action has been requested for this player.
                if app.player_id() == player_id {
                    info!(
                        "Player {} request action: {} {:?}",
                        player_id, min_raise, actions
                    );
                    self.min_raise = *min_raise;
                    self.actions_req = actions.clone();
                }
            }
            _ => {}
        }
    }

    /// Returns a reference to the players.
    pub fn players(&self) -> &[Player] {
        &self.players
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

                // Do not override cards for local player as these are assigned at
                // players cards dealing.
                if pos != 0 {
                    player.cards = update.cards;
                }

                info!("Updated player {player:?}");
            }
        }
    }
}
