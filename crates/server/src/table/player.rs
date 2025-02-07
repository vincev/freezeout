// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table player types.
use rand::seq::SliceRandom;
use std::{cmp::Ordering, time::Instant};
use tokio::sync::mpsc;

use freezeout_core::{
    crypto::PeerId,
    message::{PlayerAction, SignedMessage},
    poker::{Chips, PlayerCards},
};

use super::TableMessage;

/// A table player state.
#[derive(Debug)]
pub struct Player {
    /// The player peer id.
    pub player_id: PeerId,
    /// The channel to send messages to this player connection.
    pub table_tx: mpsc::Sender<TableMessage>,
    /// This playe nickname.
    pub nickname: String,
    /// This player chips.
    pub chips: Chips,
    /// The player bet amount.
    pub bet: Chips,
    /// The last player action.
    pub action: PlayerAction,
    /// The player action timer.
    pub action_timer: Option<Instant>,
    /// This player cards that are visible to all other players.
    pub public_cards: PlayerCards,
    /// This player private cards.
    pub hole_cards: PlayerCards,
    /// This player is active in the hand.
    pub is_active: bool,
    /// The player has the button.
    pub has_button: bool,
}

impl Player {
    /// Creates a new player.
    pub fn new(
        player_id: PeerId,
        nickname: String,
        chips: Chips,
        table_tx: mpsc::Sender<TableMessage>,
    ) -> Self {
        Self {
            player_id,
            table_tx,
            nickname,
            chips,
            bet: Chips::default(),
            action: PlayerAction::None,
            action_timer: None,
            public_cards: PlayerCards::None,
            hole_cards: PlayerCards::None,
            is_active: true,
            has_button: false,
        }
    }

    /// Send a message to this player connection.
    pub async fn send(&self, msg: SignedMessage) {
        let _ = self.table_tx.send(TableMessage::Send(msg)).await;
    }

    /// Updates this player bets to the given chips amount.
    pub fn bet(&mut self, action: PlayerAction, chips: Chips) {
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

    /// Sets this player in fold state.
    pub fn fold(&mut self) {
        self.is_active = false;
        self.action = PlayerAction::Fold;
        self.hole_cards = PlayerCards::None;
        self.public_cards = PlayerCards::None;
        self.action_timer = None;
    }

    /// Reset state for a new hand.
    fn start_hand(&mut self) {
        self.is_active = self.chips > Chips::ZERO;
        self.has_button = false;
        self.bet = Chips::ZERO;
        self.action = PlayerAction::None;
        self.public_cards = PlayerCards::None;
        self.hole_cards = PlayerCards::None;
    }

    /// Set state on hand end.
    fn end_hand(&mut self) {
        self.action = PlayerAction::None;
        self.action_timer = None;
    }
}

/// The table players state.
#[derive(Debug, Default)]
pub struct PlayersState {
    players: Vec<Player>,
    active_player: Option<usize>,
}

impl PlayersState {
    /// Adds a player to the table.
    pub fn join(&mut self, player: Player) {
        self.players.push(player);
    }

    /// Remove all players.
    pub fn clear(&mut self) {
        self.players.clear();
        self.active_player = None;
    }

    /// Removes a player from the table.
    pub fn leave(&mut self, player_id: &PeerId) -> Option<Player> {
        if let Some(pos) = self.players.iter().position(|p| &p.player_id == player_id) {
            let player = self.players.remove(pos);

            let count_active = self.count_active();
            // Adjust active_player index.
            if count_active == 0 {
                self.active_player = None;
            } else if count_active == 1 {
                self.active_player = self.players.iter().position(|p| p.is_active);
            } else if let Some(active_player) = self.active_player.as_mut() {
                // If we removed active player activate the next one, there must be
                // one as count_active > 1.
                match pos.cmp(active_player) {
                    Ordering::Less => {
                        // Adjust active player if the player leaving came before it.
                        *active_player -= 1;
                    }
                    Ordering::Equal => {
                        // Adjust index if we removed last element.
                        if pos == self.players.len() {
                            *active_player = 0;
                        }

                        loop {
                            if self.players[*active_player].is_active {
                                break;
                            }

                            *active_player = (*active_player + 1) % self.players.len();
                        }
                    }
                    _ => {}
                }
            }

            Some(player)
        } else {
            None
        }
    }

    /// Shuffles the players seats.
    pub fn shuffle_seats(&mut self) {
        let mut rng = rand::thread_rng();
        self.players.shuffle(&mut rng);
    }

    /// Returns total number of players.
    pub fn count(&self) -> usize {
        self.players.len()
    }

    /// Returns the number of active players.
    pub fn count_active(&self) -> usize {
        self.players.iter().filter(|p| p.is_active).count()
    }

    /// Returns the number of player in the hand who have chips.
    pub fn count_active_with_chips(&self) -> usize {
        self.players
            .iter()
            .filter(|p| p.is_active && p.chips > Chips::ZERO)
            .count()
    }

    /// Returns the number of player who have chips.
    pub fn count_with_chips(&self) -> usize {
        self.players
            .iter()
            .filter(|p| p.chips > Chips::ZERO)
            .count()
    }

    /// Returns the active player.
    pub fn active_player(&mut self) -> Option<&mut Player> {
        self.active_player
            .and_then(|idx| self.players.get_mut(idx))
            .filter(|p| p.is_active)
    }

    /// Check if this player is active.
    pub fn is_active(&self, player_id: &PeerId) -> bool {
        self.active_player
            .and_then(|idx| self.players.get(idx))
            .map(|p| &p.player_id == player_id)
            .unwrap_or(false)
    }

    /// Returns an iterator to all players.
    pub fn iter(&self) -> impl Iterator<Item = &Player> {
        self.players.iter()
    }

    /// Returns a mutable iterator to all players.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.iter_mut()
    }

    /// Activate the next player if there is more than one active player.
    pub fn activate_next_player(&mut self) {
        if self.count_active() > 0 && self.active_player.is_some() {
            loop {
                let active_player = self.active_player.get_or_insert_default();
                *active_player = (*active_player + 1) % self.players.len();
                if self.players[*active_player].is_active {
                    break;
                }
            }
        }
    }

    /// Set state for a new hand.
    pub fn start_hand(&mut self) {
        for player in &mut self.players {
            player.start_hand();
        }

        if self.count_active() > 1 {
            // Rotate players so that the first player becomes the button.
            loop {
                self.players.rotate_left(1);
                if self.players[0].is_active {
                    // Checked above there are at least 2 active players, go back and
                    // set the button.
                    for p in self.players.iter_mut().rev() {
                        if p.is_active {
                            p.has_button = true;
                            break;
                        }
                    }

                    break;
                }
            }

            self.active_player = Some(0);
        } else {
            self.active_player = None;
        }
    }

    /// Starts a new round.
    pub fn start_round(&mut self) {
        self.active_player = None;

        for (idx, p) in self.players.iter().enumerate() {
            if p.chips > Chips::ZERO && p.is_active {
                self.active_player = Some(idx);
                return;
            }
        }
    }

    /// The hand has ended disable any active player.
    pub fn end_hand(&mut self) {
        self.active_player = None;
        self.players.iter_mut().for_each(Player::end_hand);
    }

    /// Remove players that run out of chips.
    pub fn remove_with_no_chips(&mut self) {
        self.players.retain(|p| p.chips > Chips::ZERO);
    }
}
