// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table state types.
use ahash::{AHashMap, AHashSet};
use anyhow::{bail, Result};
use log::{error, info};
use rand::{rngs::StdRng, SeedableRng};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, time};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::{HandPayoff, Message, PlayerAction, PlayerUpdate, SignedMessage},
    poker::{Card, Chips, Deck, HandValue, PlayerCards, TableId},
};

use crate::db::Db;

use super::{
    player::{Player, PlayersState},
    TableMessage,
};

/// The hand state.
#[derive(Debug)]
enum HandState {
    /// The table is waiting for players to join before starting the game.
    WaitForPlayers,
    /// Start the game.
    StartGame,
    /// Start the hand, collect blinds and deal cards.
    StartHand,
    /// Handle preflop betting.
    PreflopBetting,
    /// Handle flop betting.
    FlopBetting,
    /// Handle turn betting.
    TurnBetting,
    /// Handle river players action.
    RiverBetting,
    /// Showdown.
    Showdown,
    /// The hand has ended.
    EndHand,
    /// The game has ended with a winner.
    EndGame,
}

/// A pot that contains players bets.
#[derive(Debug, Default)]
struct Pot {
    players: AHashSet<PeerId>,
    chips: Chips,
}

/// Internal table state.
#[derive(Debug)]
pub struct State {
    table_id: TableId,
    seats: usize,
    sk: Arc<SigningKey>,
    db: Db,
    hand_state: HandState,
    small_blind: Chips,
    big_blind: Chips,
    players: PlayersState,
    deck: Deck,
    last_bet: Chips,
    min_raise: Chips,
    pots: Vec<Pot>,
    board: Vec<Card>,
    new_hand_start_time: Option<Instant>,
    rng: StdRng,
}

impl State {
    const ACTION_TIMEOUT: Duration = Duration::from_secs(15);
    const NEW_HAND_TIMEOUT: Duration = Duration::from_secs(10);

    /// Create a new state.
    pub fn new(table_id: TableId, seats: usize, sk: Arc<SigningKey>, db: Db) -> Self {
        Self::with_rng(table_id, seats, sk, db, StdRng::from_entropy())
    }

    /// Create a new state with user initialized randomness.
    fn with_rng(
        table_id: TableId,
        seats: usize,
        sk: Arc<SigningKey>,
        db: Db,
        mut rng: StdRng,
    ) -> Self {
        Self {
            table_id,
            seats,
            sk,
            db,
            hand_state: HandState::WaitForPlayers,
            small_blind: 10_000.into(),
            big_blind: 20_000.into(),
            players: PlayersState::default(),
            deck: Deck::new_and_shuffled(&mut rng),
            last_bet: Chips::ZERO,
            min_raise: Chips::ZERO,
            pots: vec![Pot::default()],
            board: Vec::default(),
            new_hand_start_time: None,
            rng,
        }
    }

    /// A player tries to join the table.
    pub async fn join(
        &mut self,
        player_id: &PeerId,
        nickname: &str,
        join_chips: Chips,
        table_tx: mpsc::Sender<TableMessage>,
    ) -> Result<()> {
        if !matches!(self.hand_state, HandState::WaitForPlayers) {
            bail!("Hand in progress");
        }

        if self.players.count() == self.seats {
            bail!("Table full");
        }

        if self.players.iter().any(|p| &p.player_id == player_id) {
            bail!("Player has already joined");
        }

        // Add new player to the table.
        let join_player = Player::new(
            player_id.clone(),
            nickname.to_string(),
            join_chips,
            table_tx,
        );

        // Send a table joined confirmation to the player who joined.
        let msg = Message::TableJoined {
            table_id: self.table_id,
            chips: join_player.chips,
            seats: self.seats as u8,
        };
        let smsg = SignedMessage::new(&self.sk, msg);
        let _ = join_player.table_tx.send(TableMessage::Send(smsg)).await;

        // Send joined message for each player at the table to the new player.
        for player in self.players.iter() {
            let msg = Message::PlayerJoined {
                player_id: player.player_id.clone(),
                nickname: player.nickname.clone(),
                chips: player.chips,
            };
            let smsg = SignedMessage::new(&self.sk, msg);
            let _ = join_player.table_tx.send(TableMessage::Send(smsg)).await;
        }

        // Tell all players at the table that a player joined. Note that because the
        // player has not beed added to the table yet it won't get the broadcast.
        let msg = Message::PlayerJoined {
            player_id: player_id.clone(),
            nickname: nickname.to_string(),
            chips: join_player.chips,
        };
        self.broadcast(msg).await;

        // Add new player to the table.
        self.players.join(join_player);

        info!("Player {player_id} joined table {}", self.table_id);

        // If all seats are full start the game.
        if self.players.count() == self.seats {
            self.enter_start_game().await;
        }

        Ok(())
    }

    /// A player leaves the table.
    pub async fn leave(&mut self, player_id: &PeerId) {
        let active_is_leaving = self.players.is_active(player_id);
        if let Some(player) = self.players.leave(player_id) {
            // Store the player bets into the pot.
            if let Some(pot) = self.pots.last_mut() {
                pot.chips += player.bet;
            }

            // Tell the other players this player has left.
            let msg = Message::PlayerLeft(player_id.clone());
            self.broadcast(msg).await;

            if self.players.count_active() < 2 {
                self.enter_end_hand().await;
                return;
            }

            if active_is_leaving {
                self.request_action().await;
            }
        }
    }

    /// Handle a message from a player.
    pub async fn message(&mut self, msg: SignedMessage) {
        match msg.message() {
            Message::ActionResponse { action, amount } => {
                if let Some(player) = self.players.active_player() {
                    // Only process responses coming from active player.
                    if player.player_id == msg.sender() {
                        player.action = *action;
                        player.action_timer = None;

                        match action {
                            PlayerAction::Fold => {
                                player.fold();
                            }
                            PlayerAction::Call => {
                                player.bet(*action, self.last_bet);
                            }
                            PlayerAction::Check => {}
                            PlayerAction::Bet | PlayerAction::Raise => {
                                let amount = *amount.min(&(player.bet + player.chips));
                                self.min_raise = (amount - self.last_bet).max(self.min_raise);
                                self.last_bet = amount.max(self.last_bet);
                                player.bet(*action, amount);
                            }
                            _ => {}
                        }

                        self.action_update().await;
                    }
                }
            }
            Message::Error(e) => error!("Error {e}"),
            _ => {}
        }
    }

    pub async fn tick(&mut self) {
        if let Some(dt) = self.new_hand_start_time {
            if dt.elapsed() >= Self::NEW_HAND_TIMEOUT {
                self.new_hand_start_time = None;
                self.enter_start_hand().await;
            }
        }

        // Check if there is any player with an active timer.
        if self.players.iter().any(|p| p.action_timer.is_some()) {
            let player = self
                .players
                .iter_mut()
                .find(|p| p.action_timer.is_some())
                .unwrap();

            // If timer has expired fold otherwise broadcast timer update.
            if player.action_timer.unwrap().elapsed() > Self::ACTION_TIMEOUT {
                player.fold();
                self.action_update().await;
            } else {
                self.broadcast_game_update().await;
            }
        }
    }

    async fn action_update(&mut self) {
        self.players.activate_next_player();

        if self.is_round_complete() {
            self.next_round().await;
        } else {
            self.broadcast_game_update().await;
            self.request_action().await;
        }
    }

    async fn enter_start_game(&mut self) {
        self.hand_state = HandState::StartGame;

        // Shuffle seats before starting the game.
        self.players.shuffle_seats(&mut self.rng);

        // Tell players to update their seats order.
        let seats = self.players.iter().map(|p| p.player_id.clone()).collect();
        self.broadcast(Message::StartGame(seats)).await;

        self.enter_start_hand().await;
    }

    /// Start a new hand.
    async fn enter_start_hand(&mut self) {
        self.hand_state = HandState::StartHand;

        self.players.start_hand();

        // If there are fewer than 2 active players end the game.
        if self.players.count_active() < 2 {
            self.enter_end_hand().await;
            return;
        }

        // Pay small and big blind.
        if let Some(player) = self.players.active_player() {
            player.bet(PlayerAction::SmallBlind, self.small_blind);
        };

        self.players.activate_next_player();

        if let Some(player) = self.players.active_player() {
            player.bet(PlayerAction::BigBlind, self.big_blind);
        };

        self.last_bet = self.big_blind;
        self.min_raise = self.big_blind;

        // Create a new deck.
        self.deck = Deck::new_and_shuffled(&mut self.rng);

        // Clear board.
        self.board.clear();

        // Reset pots.
        self.pots = vec![Pot::default()];

        // Tell clients to prepare for a new hand.
        self.broadcast(Message::StartHand).await;

        // Deal cards to each player.
        for player in self.players.iter_mut() {
            if player.is_active {
                player.public_cards = PlayerCards::Covered;

                // Sort cards for the UI.
                let (c1, c2) = (self.deck.deal(), self.deck.deal());
                player.hole_cards = if c1.rank() < c2.rank() {
                    PlayerCards::Cards(c1, c2)
                } else {
                    PlayerCards::Cards(c2, c1)
                };
            } else {
                player.public_cards = PlayerCards::None;
                player.hole_cards = PlayerCards::None;
            }
        }

        // Tell clients to update all players state.
        self.broadcast_game_update().await;

        // Deal the cards to each player.
        for player in self.players.iter() {
            if let PlayerCards::Cards(c1, c2) = player.hole_cards {
                let msg = Message::DealCards(c1, c2);
                let smsg = SignedMessage::new(&self.sk, msg);
                player.send(smsg).await;
            }
        }

        self.players.activate_next_player();
        self.enter_preflop_betting().await;
    }

    async fn enter_preflop_betting(&mut self) {
        self.hand_state = HandState::PreflopBetting;
        self.request_action().await;
    }

    async fn enter_deal_flop(&mut self) {
        for _ in 1..=3 {
            self.board.push(self.deck.deal());
        }

        self.hand_state = HandState::FlopBetting;
        self.start_round().await;
    }

    async fn enter_deal_turn(&mut self) {
        self.board.push(self.deck.deal());

        self.hand_state = HandState::TurnBetting;
        self.start_round().await;
    }

    async fn enter_deal_river(&mut self) {
        self.board.push(self.deck.deal());

        self.hand_state = HandState::RiverBetting;
        self.start_round().await;
    }

    async fn enter_showdown(&mut self) {
        self.hand_state = HandState::Showdown;

        self.update_pots();

        for player in self.players.iter_mut() {
            player.bet = Chips::ZERO;
            player.action = PlayerAction::None;
            if player.is_active {
                player.public_cards = player.hole_cards;
            }
        }

        self.enter_end_hand().await;
    }

    async fn enter_end_hand(&mut self) {
        self.hand_state = HandState::EndHand;

        self.update_pots();
        let winners = self.pay_bets();

        // Update players and broadcast update to all players.
        self.players.end_hand();
        self.broadcast_game_update().await;
        self.broadcast(Message::EndHand { payoffs: winners }).await;

        // End game or set timer to start new hand.
        if self.players.count_with_chips() < 2 {
            self.enter_end_game().await;
        } else {
            // Set timer for starting a new hand.
            self.new_hand_start_time = Some(Instant::now());

            // Wait before removing players.
            time::sleep(Self::NEW_HAND_TIMEOUT).await;

            // All players that run out of chips must leave the table.
            for player in self.players.iter() {
                if player.chips == Chips::ZERO {
                    let _ = player.table_tx.send(TableMessage::LeaveTable).await;

                    let msg = Message::PlayerLeft(player.player_id.clone());
                    self.broadcast(msg).await;
                }
            }

            self.players.remove_with_no_chips();
        }
    }

    async fn enter_end_game(&mut self) {
        self.hand_state = HandState::EndGame;

        // Wait some time to give time to player to see the end game.
        time::sleep(Duration::from_secs(5)).await;

        // Pay the players and tell them the game has finished and leave the table.
        for player in self.players.iter() {
            let _ = player.table_tx.send(TableMessage::LeaveTable).await;
            let res = self
                .db
                .pay_to_player(player.player_id.clone(), player.chips)
                .await;
            if let Err(e) = res {
                error!("Db players update failed {e}");
            }
        }

        self.players.clear();
        self.hand_state = HandState::WaitForPlayers;
    }

    fn pay_bets(&mut self) -> Vec<HandPayoff> {
        let mut winners = AHashMap::new();

        match self.players.count_active() {
            1 => {
                // If one player left gets all the chips.
                if let Some(player) = self.players.active_player() {
                    for pot in self.pots.drain(..) {
                        player.chips += pot.chips;

                        winners
                            .entry(player.player_id.clone())
                            .or_insert_with(|| HandPayoff {
                                player_id: player.player_id.clone(),
                                chips: Chips::ZERO,
                                cards: Vec::default(),
                            })
                            .chips += pot.chips;
                    }
                }
            }
            n if n > 1 => {
                // With more than 1 player we need to compare hands for each pot
                for pot in self.pots.drain(..) {
                    // Find the winner amongst all the active players in the pot.
                    let winner = self
                        .players
                        .iter_mut()
                        .filter(|p| p.is_active && pot.players.contains(&p.player_id))
                        .filter_map(|p| match p.hole_cards {
                            PlayerCards::None | PlayerCards::Covered => None,
                            PlayerCards::Cards(c1, c2) => Some((p, c1, c2)),
                        })
                        .map(|(p, c1, c2)| {
                            let mut cards = vec![c1, c2];
                            cards.extend_from_slice(&self.board);
                            let hv = HandValue::eval(&cards);
                            (p, hv)
                        })
                        .max_by(|p1, p2| p1.1.cmp(&p2.1));

                    if let Some((p, hv)) = winner {
                        p.chips += pot.chips;

                        // Sort by rank for the UI.
                        let mut cards = hv.hand().to_vec();
                        cards.sort_by_key(|c| c.rank());

                        winners
                            .entry(p.player_id.clone())
                            .or_insert_with(|| HandPayoff {
                                player_id: p.player_id.clone(),
                                chips: Chips::ZERO,
                                cards,
                            })
                            .chips += pot.chips;
                    }
                }
            }
            _ => {}
        }

        winners.into_values().collect()
    }

    /// Checks if all players in the hand have acted.
    fn is_round_complete(&self) -> bool {
        if self.players.count_active() < 2 {
            return true;
        }

        for player in self.players.iter() {
            // If a player didn't match the last bet and is not all-in then the
            // player has to act and the round is not complete.
            if player.is_active && player.bet < self.last_bet && player.chips > Chips::ZERO {
                return false;
            }
        }

        // Only one player has chips all others are all in.
        if self.players.count_active_with_chips() < 2 {
            return true;
        }

        for player in self.players.iter() {
            if player.is_active {
                // If a player didn't act the round is not complete.
                match player.action {
                    PlayerAction::None | PlayerAction::SmallBlind | PlayerAction::BigBlind
                        if player.chips > Chips::ZERO =>
                    {
                        return false
                    }
                    _ => {}
                }
            }
        }

        true
    }

    async fn next_round(&mut self) {
        if self.players.count_active() < 2 {
            self.enter_end_hand().await;
            return;
        }

        while self.is_round_complete() {
            match self.hand_state {
                HandState::PreflopBetting => self.enter_deal_flop().await,
                HandState::FlopBetting => self.enter_deal_turn().await,
                HandState::TurnBetting => self.enter_deal_river().await,
                HandState::RiverBetting => {
                    self.enter_showdown().await;
                    return;
                }
                _ => {}
            }
        }
    }

    async fn start_round(&mut self) {
        self.update_pots();

        for player in self.players.iter_mut() {
            player.bet = Chips::ZERO;
            player.action = PlayerAction::None;
        }

        self.last_bet = Chips::ZERO;
        self.min_raise = self.big_blind;

        self.players.start_round();

        self.broadcast_game_update().await;
        self.request_action().await;
    }

    fn update_pots(&mut self) {
        // Updates pots if there is a bet.
        if self.last_bet > Chips::ZERO {
            // Move bets to pots.
            loop {
                // Find minimum bet in case a player went all in.
                let min_bet = self
                    .players
                    .iter()
                    .filter(|p| p.bet > Chips::ZERO)
                    .map(|p| p.bet)
                    .min()
                    .unwrap_or_default();

                if min_bet == Chips::ZERO {
                    break;
                }

                let mut went_all_in = false;
                for player in self.players.iter_mut() {
                    let pot = self.pots.last_mut().unwrap();
                    if player.bet > Chips::ZERO {
                        player.bet -= min_bet;
                        pot.chips += min_bet;

                        if !pot.players.contains(&player.player_id) {
                            pot.players.insert(player.player_id.clone());
                        }

                        went_all_in = player.chips == Chips::ZERO;
                    }
                }

                if went_all_in {
                    self.pots.push(Pot::default());
                }
            }
        }
    }

    /// Broadcast a game state update to all connected players.
    async fn broadcast_game_update(&self) {
        let players = self
            .players
            .iter()
            .map(|p| {
                let action_timer = p.action_timer.map(|t| {
                    Self::ACTION_TIMEOUT
                        .saturating_sub(t.elapsed())
                        .as_secs_f32() as u16
                });

                PlayerUpdate {
                    player_id: p.player_id.clone(),
                    chips: p.chips,
                    bet: p.bet,
                    action: p.action,
                    action_timer,
                    cards: p.public_cards,
                    has_button: p.has_button,
                    is_active: p.is_active,
                }
            })
            .collect();

        let pot = self
            .pots
            .iter()
            .map(|p| p.chips)
            .fold(Chips::ZERO, |acc, c| acc + c);

        let msg = Message::GameUpdate {
            players,
            board: self.board.clone(),
            pot,
        };
        let smsg = SignedMessage::new(&self.sk, msg);
        for player in self.players.iter() {
            player.send(smsg.clone()).await;
        }
    }

    /// Request action to the active player.
    async fn request_action(&mut self) {
        if let Some(player) = self.players.active_player() {
            let mut actions = vec![PlayerAction::Fold];

            if player.bet == self.last_bet {
                actions.push(PlayerAction::Check);
            }

            if player.bet < self.last_bet {
                actions.push(PlayerAction::Call);
            }

            if self.last_bet == Chips::ZERO {
                actions.push(PlayerAction::Bet);
            }

            if player.chips + player.bet > self.last_bet && self.last_bet > Chips::ZERO {
                actions.push(PlayerAction::Raise);
            }

            player.action_timer = Some(Instant::now());

            let msg = Message::ActionRequest {
                player_id: player.player_id.clone(),
                min_raise: self.min_raise + self.last_bet,
                big_blind: self.big_blind,
                actions,
            };

            self.broadcast(msg).await;
        }
    }

    /// Broadcast a message to all players at the table.
    async fn broadcast(&self, msg: Message) {
        let smsg = SignedMessage::new(&self.sk, msg);
        for player in self.players.iter() {
            player.send(smsg.clone()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const JOIN_CHIPS: Chips = Chips::new(100_000);

    struct TestPlayer {
        p: Player,
        rx: mpsc::Receiver<TableMessage>,
        sk: SigningKey,
    }

    impl TestPlayer {
        fn new(chips: Chips) -> Self {
            let sk = SigningKey::default();
            let peer_id = sk.verifying_key().peer_id();
            let (tx, rx) = mpsc::channel(64);
            let p = Player::new(peer_id.clone(), peer_id.digits(), chips, tx);
            Self { p, rx, sk }
        }

        fn rx(&mut self) -> Option<TableMessage> {
            self.rx.try_recv().ok()
        }

        fn msg(&self, msg: Message) -> SignedMessage {
            SignedMessage::new(&self.sk, msg)
        }

        fn id(&self) -> &PeerId {
            &self.p.player_id
        }
    }

    macro_rules! assert_message {
        ($player:expr, $pattern:pat $(if $guard:expr)?) => {
            let msg = $player.rx().expect("No message found");
            match msg {
                TableMessage::Send(msg) => match msg.message() {
                    $pattern $(if $guard)? => true,
                    msg => panic!("Unexpected message {msg:?}"),
                }
                msg => panic!("Unexpected table message {msg:?}"),
            }
        };
        ($player:expr, $pattern:pat, $closure:expr) => {
            let msg = $player.rx().expect("No message found");
            match msg {
                TableMessage::Send(msg) => match msg.message() {
                    $pattern => $closure(),
                    msg => panic!("Unexpected message {msg:?}"),
                }
                msg => panic!("Unexpected table message {msg:?}"),
            }
        };
}

    struct TestState {
        state: State,
        players: Vec<TestPlayer>,
    }

    impl TestState {
        // Creates a `State` with seeded randomness and memory database.
        fn new(seats: usize) -> Self {
            let rng = StdRng::seed_from_u64(121);
            let db = Db::open_in_memory().unwrap();
            let sk = Arc::new(SigningKey::default());
            let state = State::with_rng(TableId::new_id(), seats, sk, db, rng);
            let tp = (0..seats)
                .map(|_| TestPlayer::new(Chips::new(1_000_000)))
                .collect();
            Self { state, players: tp }
        }

        async fn test_start_game(&mut self) {
            for p in self.players.iter_mut() {
                // A player joins a table.
                self.state
                    .join(
                        &p.p.player_id,
                        &p.p.nickname,
                        JOIN_CHIPS,
                        p.p.table_tx.clone(),
                    )
                    .await
                    .expect("Player should be able to join");

                // After joining a player should get a TableJoined message.
                assert_message!(p, Message::TableJoined { .. });
            }

            // List of player ids from the test players.
            let player_ids = self
                .players
                .iter()
                .map(|p| p.id().clone())
                .collect::<Vec<_>>();

            // After all players joined each player should have received a player
            // joined for each other player at the table.
            for p in self.players.iter_mut() {
                for id in &player_ids {
                    // Skip itself.
                    if p.id() != id {
                        assert_message!(p, Message::PlayerJoined { player_id: p, .. } if p == id);
                    }
                }
            }

            // Before starting the game the seats are shuffled and a StartGame
            // message with the new seats is sent to each player. Check that shuffled
            // seats id are different from the test players id.
            for p in self.players.iter_mut() {
                assert_message!(p, Message::StartGame(seats) if seats != &player_ids);
            }
        }

        // Test a start hand, this should be called after test_start_game.
        async fn test_start_hand(&mut self) {
            // Before a new hand starts all players get a StartHand message.
            for p in self.players.iter_mut() {
                assert_message!(p, Message::StartHand);
            }

            // The small blind and big blind players pay the blinds.
            let sb = &self.state.players.player(0);
            assert_eq!(sb.bet, self.state.small_blind);
            assert_eq!(sb.chips, JOIN_CHIPS - self.state.small_blind);
            assert!(matches!(sb.action, PlayerAction::SmallBlind));

            let bb = &self.state.players.player(1);
            assert_eq!(bb.bet, self.state.big_blind);
            assert_eq!(bb.chips, JOIN_CHIPS - self.state.big_blind);
            assert!(matches!(bb.action, PlayerAction::BigBlind));

            // The next playe to act is after the big blind.
            let action_player = 2 % self.state.players.count();
            let action_id = self.state.players.player(action_player).player_id.clone();

            // After players paid the blinds all players should get a game update so
            // that they can update the UI and then the hole cards are dealt.
            for p in self.players.iter_mut() {
                assert_message!(p, Message::GameUpdate { players, .. }, || {
                    assert_eq!(players[0].bet, self.state.small_blind);
                    assert_eq!(players[1].bet, self.state.big_blind);
                });

                assert_message!(p, Message::DealCards(_, _));

                // After the blinds each player get an ActionRequest with the player id
                // and blinds of the player that must act.
                assert_message!(p, Message::ActionRequest { player_id, .. }, || {
                    assert_eq!(player_id, &action_id);
                });
            }
        }

        // Send an action from the current active player.
        async fn send_action(&mut self, msg: Message) {
            let active_id = self
                .state
                .players
                .active_player()
                .expect("No active player")
                .player_id
                .clone();

            // Find the sender player.
            for p in self.players.iter_mut() {
                if p.id() == &active_id {
                    let msg = p.msg(msg);
                    self.state.message(msg).await;
                    break;
                }
            }
        }
    }

    #[tokio::test]
    async fn all_players_go_all_in() {
        const NUM_SEATS: usize = 3;
        let mut state = TestState::new(NUM_SEATS);
        state.test_start_game().await;
        state.test_start_hand().await;

        // First player to act goes all in.
        state
            .send_action(Message::ActionResponse {
                action: PlayerAction::Bet,
                amount: JOIN_CHIPS,
            })
            .await;

        // All players get an game update with the player action and an action
        // request for the next player.
        for p in state.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Next player calls.
        state
            .send_action(Message::ActionResponse {
                action: PlayerAction::Call,
                amount: Chips::ZERO,
            })
            .await;

        for p in state.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Last player calls.
        state
            .send_action(Message::ActionResponse {
                action: PlayerAction::Call,
                amount: Chips::ZERO,
            })
            .await;

        // All players went all in we should get the following messages.
        for p in state.players.iter_mut() {
            // All players get a game update with the flop cards.
            assert_message!(p, Message::GameUpdate { board, pot, .. }, || {
                assert_eq!(board.len(), 3);
                assert_eq!(*pot, JOIN_CHIPS + JOIN_CHIPS + JOIN_CHIPS);
            });

            // All players get an update for the turn.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 4);
            });

            // And the river.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 5);
            });

            // Showdown message with all players cards.
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                for p in players {
                    assert!(matches!(p.cards, PlayerCards::Cards(_, _)));
                }
            });

            // All players get a EndHand message with winner.
            assert_message!(p, Message::EndHand { payoffs }, || {
                // Only one payoff
                assert_eq!(payoffs.len(), 1);

                // Winner wins all chips.
                assert_eq!(payoffs[0].chips, JOIN_CHIPS + JOIN_CHIPS + JOIN_CHIPS);
            });
        }
    }
}
