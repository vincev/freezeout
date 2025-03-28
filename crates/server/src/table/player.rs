// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table player types.
use rand::prelude::*;
use std::{
    cmp::Ordering,
    time::{Duration, Instant},
};
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
    pub async fn send_message(&self, msg: SignedMessage) {
        let _ = self.table_tx.send(TableMessage::Send(msg)).await;
    }

    /// Tell the player connection handle this player has left the table.
    pub async fn send_player_left(&self) {
        let _ = self.table_tx.send(TableMessage::PlayerLeft).await;
    }

    /// Send a throttle message to this player connection.
    pub async fn send_throttle(&self, dt: Duration) {
        let _ = self.table_tx.send(TableMessage::Throttle(dt)).await;
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
    pub fn shuffle_seats<R: Rng>(&mut self, rng: &mut R) {
        self.players.shuffle(rng);
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

    /// Returns a reference to a player at the given index.
    /// Used for testing.
    #[cfg(test)]
    pub fn player(&self, idx: usize) -> &Player {
        self.players.get(idx).expect("No player at the given index")
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

#[cfg(test)]
mod tests {
    use super::*;
    use freezeout_core::crypto::SigningKey;

    fn new_player(chips: Chips) -> Player {
        let peer_id = SigningKey::default().verifying_key().peer_id();
        let (table_tx, _table_rx) = mpsc::channel(10);
        Player::new(
            peer_id.clone(),
            "Alice".to_string(),
            chips,
            table_tx.clone(),
        )
    }

    #[test]
    fn test_player_bet() {
        let init_chips = Chips::new(100_000);
        let mut player = new_player(init_chips);

        // Simple bet.
        let bet_size = Chips::new(60_000);
        player.bet(PlayerAction::Bet, bet_size);
        assert_eq!(player.bet, bet_size);
        assert_eq!(player.chips, init_chips - bet_size);
        assert!(matches!(player.action, PlayerAction::Bet));

        // The bet amount is the total bet check chips paid are the new bet minus the
        // previous bet.
        let bet_size = bet_size + Chips::new(20_000);
        player.bet(PlayerAction::Bet, bet_size);
        assert_eq!(player.bet, bet_size);
        assert_eq!(player.chips, init_chips - bet_size);

        // Start new hand reset bet chips and action.
        player.start_hand();
        assert!(matches!(player.action, PlayerAction::None));
        assert!(player.is_active);
        assert_eq!(player.bet, Chips::ZERO);
        assert_eq!(player.chips, init_chips - bet_size);

        // Bet more than remaining chips goes all in.
        let remaining_chips = player.chips;
        player.bet(PlayerAction::Bet, Chips::new(1_000_000));
        assert_eq!(player.bet, remaining_chips);
        assert_eq!(player.chips, Chips::ZERO);
    }

    #[test]
    fn test_player_fold() {
        let init_chips = Chips::new(100_000);
        let mut player = new_player(init_chips);

        player.bet(PlayerAction::Bet, Chips::new(20_000));
        player.action_timer = Some(Instant::now());

        player.fold();
        assert!(matches!(player.action, PlayerAction::Fold));
        assert!(!player.is_active);
        assert!(player.action_timer.is_none());
    }

    fn new_players_state(n: usize) -> PlayersState {
        let mut players = PlayersState::default();
        (0..n).for_each(|_| players.join(new_player(Chips::new(100_000))));
        players
    }

    #[test]
    fn player_before_active_leaves() {
        const SEATS: usize = 4;
        let mut players = new_players_state(SEATS);

        assert_eq!(players.count_active(), SEATS);
        assert!(players.active_player().is_none());

        // Make player at index 1 active.
        players.start_hand();
        players.activate_next_player();
        assert_eq!(players.active_player.unwrap(), 1);

        // Player before active leaves, the active player moved to position 0.
        let player_id = players.player(0).player_id.clone();
        assert!(players.leave(&player_id).is_some());
        assert_eq!(players.active_player.unwrap(), 0);
        assert_eq!(players.count_active(), SEATS - 1);
    }

    #[test]
    fn player_after_active_leaves() {
        const SEATS: usize = 4;
        let mut players = new_players_state(SEATS);

        assert_eq!(players.count_active(), SEATS);
        assert!(players.active_player().is_none());

        // Make player at index 1 active.
        players.start_hand();
        players.activate_next_player();
        assert_eq!(players.active_player.unwrap(), 1);

        // Player after active leaves, the active player should be the same.
        let player_id = players.player(2).player_id.clone();
        assert!(players.leave(&player_id).is_some());
        assert_eq!(players.active_player.unwrap(), 1);
        assert_eq!(players.count_active(), SEATS - 1);
    }

    #[test]
    fn active_player_leaves() {
        const SEATS: usize = 4;
        let mut players = new_players_state(SEATS);

        assert_eq!(players.count_active(), SEATS);
        assert!(players.active_player().is_none());

        // Make player at index 1 active.
        players.start_hand();
        players.activate_next_player();
        assert_eq!(players.active_player.unwrap(), 1);

        // Active leaves the next player should become active.
        let active_id = players.player(1).player_id.clone();
        let next_id = players.player(2).player_id.clone();
        assert!(players.leave(&active_id).is_some());
        assert_eq!(players.active_player.unwrap(), 1);
        assert_eq!(players.active_player().unwrap().player_id, next_id);
        assert_eq!(players.count_active(), SEATS - 1);
    }

    #[test]
    fn active_player_before_inactive_player_leaves() {
        const SEATS: usize = 4;
        let mut players = new_players_state(SEATS);

        assert_eq!(players.count_active(), SEATS);
        assert!(players.active_player().is_none());

        // Make player at index 1 active.
        players.start_hand();
        players.activate_next_player();
        assert_eq!(players.active_player.unwrap(), 1);

        // Deactivate player at index 2
        players.iter_mut().nth(2).unwrap().fold();
        assert_eq!(players.count_active(), SEATS - 1);

        // Active leaves but the player after that has folded so the next player at
        // index 3, that will move to index 2, should become active.
        let active_id = players.player(1).player_id.clone();
        let next_id = players.player(3).player_id.clone();
        assert!(players.leave(&active_id).is_some());
        assert_eq!(players.active_player.unwrap(), 2);
        assert_eq!(players.active_player().unwrap().player_id, next_id);
        assert_eq!(players.count_active(), SEATS - 2);
    }
}
