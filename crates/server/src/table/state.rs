// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table state types.
use ahash::AHashSet;
use anyhow::{Result, bail};
use log::{error, info};
use rand::{SeedableRng, rngs::StdRng};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::{HandPayoff, Message, PlayerAction, PlayerUpdate, SignedMessage},
    poker::{Card, Chips, Deck, HandValue, PlayerCards, TableId},
};

use crate::db::Db;

use super::{
    TableMessage,
    player::{Player, PlayersState},
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
    hand_count: usize,
    players: PlayersState,
    deck: Deck,
    last_bet: Chips,
    min_raise: Chips,
    pots: Vec<Pot>,
    board: Vec<Card>,
    rng: StdRng,
    new_hand_timer: Option<Instant>,
}

impl State {
    const ACTION_TIMEOUT: Duration = Duration::from_secs(15);
    const NEW_HAND_TIMEOUT: Duration = Duration::from_millis(7500);
    const START_GAME_SB: Chips = Chips::new(10_000);
    const START_GAME_BB: Chips = Chips::new(20_000);

    /// Create a new state.
    pub fn new(table_id: TableId, seats: usize, sk: Arc<SigningKey>, db: Db) -> Self {
        Self::with_rng(table_id, seats, sk, db, StdRng::from_os_rng())
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
            small_blind: Self::START_GAME_SB,
            big_blind: Self::START_GAME_BB,
            hand_count: 0,
            players: PlayersState::default(),
            deck: Deck::shuffled(&mut rng),
            last_bet: Chips::ZERO,
            min_raise: Chips::ZERO,
            pots: vec![Pot::default()],
            board: Vec::default(),
            rng,
            new_hand_timer: None,
        }
    }

    /// Returns true if the game at this table has started.
    pub fn has_game_started(&self) -> bool {
        !matches!(self.hand_state, HandState::WaitForPlayers)
    }

    /// Checks if the table has any players left.
    pub fn is_empty(&self) -> bool {
        self.players.count() == 0
    }

    /// A player tries to join the table.
    pub async fn try_join(
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
        self.broadcast_message(msg).await;

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
            self.broadcast_message(msg).await;

            // Notify the handler this player has left the table.
            player.send_player_left().await;

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
        if let Message::ActionResponse { action, amount } = msg.message() {
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
    }

    pub async fn tick(&mut self) {
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

        // Check if it is time to start a new hand.
        if let Some(timer) = &self.new_hand_timer {
            if timer.elapsed() > Self::NEW_HAND_TIMEOUT {
                self.new_hand_timer = None;
                self.enter_start_hand().await;
            }
        }
    }

    async fn action_update(&mut self) {
        self.players.activate_next_player();
        self.broadcast_game_update().await;

        if self.is_round_complete() {
            self.next_round().await;
        } else {
            self.request_action().await;
        }
    }

    async fn enter_start_game(&mut self) {
        self.hand_state = HandState::StartGame;

        // Shuffle seats before starting the game.
        self.players.shuffle_seats(&mut self.rng);

        // Tell players to update their seats order.
        let seats = self.players.iter().map(|p| p.player_id.clone()).collect();
        self.broadcast_message(Message::StartGame(seats)).await;

        self.enter_start_hand().await;
    }

    /// Start a new hand.
    async fn enter_start_hand(&mut self) {
        self.hand_state = HandState::StartHand;

        self.players.start_hand();

        // If there are fewer than 2 active players end the game.
        if self.players.count_active() < 2 {
            self.enter_end_game().await;
            return;
        }

        self.update_blinds();

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
        self.deck = Deck::shuffled(&mut self.rng);

        // Clear board.
        self.board.clear();

        // Reset pots.
        self.pots = vec![Pot::default()];

        // Tell clients to prepare for a new hand.
        self.broadcast_message(Message::StartHand).await;

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
                player.send_message(smsg).await;
            }
        }

        self.enter_preflop_betting().await;
    }

    async fn enter_preflop_betting(&mut self) {
        self.hand_state = HandState::PreflopBetting;
        self.action_update().await;
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

        for player in self.players.iter_mut() {
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
        self.broadcast_game_update().await;
        // Give time to the UI to look at the updated pot and board.
        self.broadcast_throttle(Duration::from_millis(1500)).await;

        let winners = self.pay_bets();

        // Update players and broadcast update to all players.
        self.players.end_hand();
        self.broadcast_message(Message::EndHand {
            payoffs: winners,
            board: self.board.clone(),
            cards: self
                .players
                .iter()
                .map(|p| (p.player_id.clone(), p.public_cards))
                .collect(),
        })
        .await;

        // End game if only player has chips or move to next hand.
        if self.players.count_with_chips() < 2 {
            self.enter_end_game().await;
        } else {
            // All players that run out of chips must leave the table before the
            // start of a new hand.
            for player in self.players.iter() {
                if player.chips == Chips::ZERO {
                    // Notify the client that this player has left the table.
                    let _ = player.table_tx.send(TableMessage::PlayerLeft).await;

                    let msg = Message::PlayerLeft(player.player_id.clone());
                    self.broadcast_message(msg).await;
                }
            }

            self.players.remove_with_no_chips();
            self.new_hand_timer = Some(Instant::now());
        }
    }

    async fn enter_end_game(&mut self) {
        // Give time to the UI to look at winning results before ending the game.
        self.broadcast_throttle(Duration::from_millis(4500)).await;

        self.hand_state = HandState::EndGame;

        for player in self.players.iter() {
            // Pay the winning player.
            let res = self
                .db
                .pay_to_player(player.player_id.clone(), player.chips)
                .await;
            if let Err(e) = res {
                error!("Db players update failed {e}");
            }

            // Notify the client that this player has left the table.
            let _ = player.table_tx.send(TableMessage::PlayerLeft).await;
        }

        self.players.clear();

        // Reset hand count for next game.
        self.hand_count = 0;

        // Wait for players to join.
        self.hand_state = HandState::WaitForPlayers;
    }

    fn update_blinds(&mut self) {
        let multiplier = (1 << (self.hand_count / 4).min(4)) as u32;
        if multiplier < 16 {
            self.small_blind = Self::START_GAME_SB * multiplier;
            self.big_blind = Self::START_GAME_BB * multiplier;
        } else {
            // Cap at 12 times initial blinds.
            self.small_blind = Self::START_GAME_SB * 12;
            self.big_blind = Self::START_GAME_BB * 12;
        }

        self.hand_count += 1;
    }

    fn pay_bets(&mut self) -> Vec<HandPayoff> {
        let mut payoffs = Vec::<HandPayoff>::new();

        match self.players.count_active() {
            1 => {
                // If one player left gets all the chips.
                if let Some(player) = self.players.active_player() {
                    for pot in self.pots.drain(..) {
                        player.chips += pot.chips;

                        if let Some(payoff) = payoffs
                            .iter_mut()
                            .find(|po| po.player_id == player.player_id)
                        {
                            payoff.chips += pot.chips;
                        } else {
                            payoffs.push(HandPayoff {
                                player_id: player.player_id.clone(),
                                chips: pot.chips,
                                cards: Vec::default(),
                                rank: String::default(),
                            });
                        }
                    }
                }
            }
            n if n > 1 => {
                // With more than 1 active player we need to compare hands for each pot
                for pot in self.pots.drain(..) {
                    // Evaluate all active players hands.
                    let mut hands = self
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
                            let (v, bh) = HandValue::eval_with_best_hand(&cards);
                            (p, v, bh)
                        })
                        .collect::<Vec<_>>();

                    // This may happen when the last pot is empty.
                    if hands.is_empty() {
                        continue;
                    }

                    // Sort descending order, winners first.
                    hands.sort_by(|p1, p2| p2.1.cmp(&p1.1));

                    // Count hands with the same value.
                    let winners_count = hands.iter().filter(|(_, v, _)| v == &hands[0].1).count();
                    let win_payoff = pot.chips / winners_count as u32;
                    let win_remainder = pot.chips % winners_count as u32;

                    for (idx, (player, v, bh)) in hands.iter_mut().take(winners_count).enumerate() {
                        // Give remaineder to first player.
                        let player_payoff = if idx == 0 {
                            win_payoff + win_remainder
                        } else {
                            win_payoff
                        };

                        player.chips += player_payoff;

                        // Sort by rank for the UI.
                        let mut cards = bh.to_vec();
                        cards.sort_by_key(|c| c.rank());

                        // If a player has already a payoff add chips to that one.
                        if let Some(payoff) = payoffs
                            .iter_mut()
                            .find(|po| po.player_id == player.player_id)
                        {
                            payoff.chips += player_payoff;
                        } else {
                            payoffs.push(HandPayoff {
                                player_id: player.player_id.clone(),
                                chips: player_payoff,
                                cards,
                                rank: v.rank().to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }

        payoffs
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
                        return false;
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

        // Give some time to watch last action and pots.
        self.broadcast_throttle(Duration::from_millis(1000)).await;

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
            player.send_message(smsg.clone()).await;
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

            if self.last_bet == Chips::ZERO && player.chips > Chips::ZERO {
                actions.push(PlayerAction::Bet);
            }

            if player.chips + player.bet > self.last_bet
                && self.last_bet > Chips::ZERO
                && player.chips > Chips::ZERO
            {
                actions.push(PlayerAction::Raise);
            }

            player.action_timer = Some(Instant::now());

            let msg = Message::ActionRequest {
                player_id: player.player_id.clone(),
                min_raise: self.min_raise + self.last_bet,
                big_blind: self.big_blind,
                actions,
            };

            self.broadcast_message(msg).await;
        }
    }

    /// Broadcast a message to all players at the table.
    async fn broadcast_message(&self, msg: Message) {
        let smsg = SignedMessage::new(&self.sk, msg);
        for player in self.players.iter() {
            player.send_message(smsg.clone()).await;
        }
    }

    /// Broadcast a throttle message to all players at the table.
    async fn broadcast_throttle(&self, dt: Duration) {
        for player in self.players.iter() {
            player.send_throttle(dt).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freezeout_core::poker::{Rank, Suit};

    struct TestPlayer {
        p: Player,
        rx: mpsc::Receiver<TableMessage>,
        sk: SigningKey,
        join_chips: Chips,
    }

    impl TestPlayer {
        fn new(join_chips: Chips) -> Self {
            let sk = SigningKey::default();
            let peer_id = sk.verifying_key().peer_id();
            let (tx, rx) = mpsc::channel(64);
            let p = Player::new(peer_id.clone(), peer_id.digits(), join_chips, tx);
            Self {
                p,
                rx,
                sk,
                join_chips,
            }
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
        ($player:expr, $pattern:pat $(, $closure:expr)?) => {
            loop {
                let msg = $player.rx().expect("No message found");
                match msg {
                    TableMessage::Send(msg) => match msg.message() {
                        $pattern => {
                            $($closure();)?
                            break;
                        }
                        msg => panic!("Unexpected message {msg:?}"),
                    },
                    TableMessage::Throttle(_) => {
                        // Ignore throttle messages while testing.
                    }
                    msg => panic!("Unexpected table message {msg:?}"),
                }
            }
        };
    }

    struct TestTable {
        state: State,
        players: Vec<TestPlayer>,
    }

    impl TestTable {
        /// Creates a `State` with seeded randomness and memory database.
        fn new(player_chips: Vec<u32>) -> Self {
            let rng = StdRng::seed_from_u64(101333);
            let db = Db::open_in_memory().unwrap();
            let sk = Arc::new(SigningKey::default());
            let state = State::with_rng(TableId::new_id(), player_chips.len(), sk, db, rng);
            let players = player_chips
                .into_iter()
                .map(|c| TestPlayer::new(Chips::new(c)))
                .collect();
            Self { state, players }
        }

        /// Start the game and test it.
        async fn test_start_game(&mut self) {
            for p in self.players.iter_mut() {
                // A player joins a table.
                self.state
                    .try_join(
                        &p.p.player_id,
                        &p.p.nickname,
                        p.join_chips,
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
                        assert_message!(p, Message::PlayerJoined { player_id, .. }, || {
                            assert_eq!(player_id, id);
                        });
                    }
                }
            }

            // Before starting the game the seats are shuffled and a StartGame
            // message with the new seats is sent to each player. Check that shuffled
            // seats id are different from the test players id.
            for p in self.players.iter_mut() {
                assert_message!(p, Message::StartGame(seats), || {
                    assert_ne!(seats, &player_ids);
                });
            }

            // Sort test players after shuffling.
            for (idx, p) in self.state.players.iter().enumerate() {
                let pos = self
                    .players
                    .iter()
                    .position(|tp| tp.p.player_id == p.player_id)
                    .unwrap();
                self.players.swap(idx, pos);
            }
        }

        /// Test a start hand, this should be called after test_start_game.
        async fn test_start_hand(&mut self) {
            // Before a new hand starts all players get a StartHand message.
            for p in self.players.iter_mut() {
                assert_message!(p, Message::StartHand);
            }

            // The small blind and big blind players pay the blinds.
            let tp = &self.players[0];
            // Use min in case join chips < small blind.
            let sb_bet = self.state.small_blind.min(tp.join_chips);

            let tp = &self.players[1];
            // Use min in case join chips < small blind.
            let bb_bet = self.state.big_blind.min(tp.join_chips);

            // After players paid the blinds all players should get a game update so
            // that they can update the UI and then the hole cards are dealt.
            for p in self.players.iter_mut() {
                assert_message!(p, Message::GameUpdate { players, .. }, || {
                    assert_eq!(players[0].bet, sb_bet);
                    assert!(matches!(players[0].action, PlayerAction::SmallBlind));

                    assert_eq!(players[1].bet, bb_bet);
                    assert!(matches!(players[1].action, PlayerAction::BigBlind));
                });

                assert_message!(p, Message::DealCards(_, _));
            }
        }

        /// Send an action from the current active player.
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

        async fn bet(&mut self, amount: Chips) {
            self.send_action(Message::ActionResponse {
                action: PlayerAction::Bet,
                amount,
            })
            .await;
        }

        async fn call(&mut self) {
            self.send_action(Message::ActionResponse {
                action: PlayerAction::Call,
                amount: Chips::ZERO,
            })
            .await;
        }

        async fn check(&mut self) {
            self.send_action(Message::ActionResponse {
                action: PlayerAction::Check,
                amount: Chips::ZERO,
            })
            .await;
        }

        async fn fold(&mut self) {
            self.send_action(Message::ActionResponse {
                action: PlayerAction::Fold,
                amount: Chips::ZERO,
            })
            .await;
        }

        /// Drain players messages for tests where we are not interested in the
        /// messages players are getting.
        fn drain_players_message(&mut self) {
            for p in self.players.iter_mut() {
                while p.rx().is_some() {}
            }
        }
    }

    #[tokio::test]
    async fn all_players_all_in() {
        const JOIN_CHIPS: u32 = 100_000;

        let mut table = TestTable::new(vec![JOIN_CHIPS, JOIN_CHIPS, JOIN_CHIPS]);
        table.test_start_game().await;
        table.test_start_hand().await;

        // Request action from first player.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // First player to act goes all in.
        table.bet(Chips::new(JOIN_CHIPS)).await;

        // All players get a game update with the player action followed by an action
        // request for the next player to act.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[2].action, PlayerAction::Bet));
            });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Next player calls.
        table.call().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[0].action, PlayerAction::Call));
            });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Last player calls.
        table.call().await;

        // All players went all in we should get the following messages.
        for p in table.players.iter_mut() {
            // BB player calls.
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                // BB playe calls.
                assert!(matches!(players[1].action, PlayerAction::Call));
            });

            // All players get a game update with the flop cards.
            assert_message!(p, Message::GameUpdate { board, pot, .. }, || {
                assert_eq!(board.len(), 3);
                assert_eq!(*pot, Chips::new(3 * JOIN_CHIPS));
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
            assert_message!(p, Message::EndHand { payoffs, .. }, || {
                // Only one payoff
                assert_eq!(payoffs.len(), 1);

                // Winner wins all chips.
                assert_eq!(payoffs[0].chips, Chips::new(300_000));
            });
        }
    }

    #[tokio::test]
    async fn two_players_one_all_in() {
        const JOIN_CHIPS: u32 = 100_000;
        const JOIN_CHIPS_SMALL: u32 = JOIN_CHIPS / 2;

        let mut table = TestTable::new(vec![JOIN_CHIPS_SMALL, JOIN_CHIPS]);
        table.test_start_game().await;
        table.test_start_hand().await;

        // Request action from first player.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // First player to act goes all in, this is the player with fewer chips.
        table.bet(Chips::new(JOIN_CHIPS_SMALL)).await;

        // All players get a game update with the player action followed by an action
        // request for the next player to act.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[0].action, PlayerAction::Bet));
            });
            assert_message!(p, Message::ActionRequest { .. });
        }

        table.call().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[1].action, PlayerAction::Call));
            });

            // New round deal flop update.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 3);
            });

            // Deal turn.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 4);
            });

            // Deal river.
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
            assert_message!(p, Message::EndHand { payoffs, .. }, || {
                // Only one payoff
                assert_eq!(payoffs.len(), 1);
                assert_eq!(payoffs[0].chips, Chips::new(100_000));
            });
        }
    }

    #[tokio::test]
    async fn three_players_one_all_in() {
        const JOIN_CHIPS: u32 = 100_000;
        const JOIN_CHIPS_SMALL: u32 = JOIN_CHIPS / 2;

        let mut table = TestTable::new(vec![JOIN_CHIPS, JOIN_CHIPS, JOIN_CHIPS_SMALL]);
        table.test_start_game().await;
        table.test_start_hand().await;

        // Request action from first player.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // First player UG goes all in, this is the player with fewer chips.
        table.bet(Chips::new(JOIN_CHIPS_SMALL)).await;

        // All players get a game update with the player action followed by an action
        // request for the next player to act.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[2].action, PlayerAction::Bet));
            });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // SB calls.
        table.call().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[0].action, PlayerAction::Call));
            });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // BB calls.
        table.call().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[1].action, PlayerAction::Call));
            });

            // New round deal flop update.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 3);
            });

            // Request action to SB.
            assert_message!(p, Message::ActionRequest { .. });
        }

        // SB check
        table.check().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[0].action, PlayerAction::Check));
            });

            assert_message!(p, Message::ActionRequest { .. });
        }

        // BB bets so we can check that the action goes back to SB as the UG players
        // is all in.
        table.bet(table.state.big_blind).await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[1].action, PlayerAction::Bet));
            });

            // Check action goes back to SB
            assert_message!(p, Message::ActionRequest { player_id, .. }, || {
                assert_eq!(player_id, &table.state.players.player(0).player_id);
            });
        }
    }

    #[tokio::test]
    async fn small_blind_all_in() {
        // Test games where the small blind chips are lower than the small blind.
        let mut table = TestTable::new(vec![20_000, 100_000]);
        // Incremebt small blind to 40000 so that is greater than player chips.

        loop {
            table.state.update_blinds();
            if table.state.small_blind == Chips::new(40_000) {
                break;
            }
        }

        table.test_start_game().await;
        table.test_start_hand().await;

        // The small blind player is all in we should go all the way to showdown.
        for p in table.players.iter_mut() {
            // Preflop game update.
            assert_message!(p, Message::GameUpdate { .. });

            // New round deal flop update.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 3);
            });

            // New round deal turn update.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 4);
            });

            // New round deal river update.
            assert_message!(p, Message::GameUpdate { board, .. }, || {
                assert_eq!(board.len(), 5);
            });

            // Pot update.
            assert_message!(p, Message::GameUpdate { pot, .. }, || {
                // Pot if the big blind plus the small blind chips that were half the
                // small blind.
                assert_eq!(*pot, table.state.big_blind + Chips::new(20_000));
            });

            // End hand.
            assert_message!(p, Message::EndHand { payoffs, .. }, || {
                assert_eq!(payoffs.len(), 1);

                // All chips go back to the BB winner.
                let payoff = &payoffs[0];
                assert_eq!(payoff.chips, table.state.big_blind + Chips::new(20_000));
            });
        }
    }

    #[tokio::test]
    async fn all_players_fold() {
        let mut table = TestTable::new(vec![100_000, 100_000, 100_000]);
        table.test_start_game().await;
        table.test_start_hand().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        let bb_player_id = table.state.players.player(1).player_id.clone();

        // First player folds.
        table.fold().await;

        // Game update with the last player action and request for next player.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[2].action, PlayerAction::Fold));
            });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Next player folds.
        table.fold().await;

        for p in table.players.iter_mut() {
            // Players get a game update where the small blind and the UTG folded.
            assert_message!(p, Message::GameUpdate { players, .. }, || {
                assert!(matches!(players[0].action, PlayerAction::Fold));
                assert!(matches!(players[2].action, PlayerAction::Fold));
            });

            // Players get an update with pot.
            assert_message!(p, Message::GameUpdate { pot, .. }, || {
                assert_eq!(*pot, table.state.big_blind + table.state.small_blind);
            });

            // Players get a EndHand message with the BB as winner.
            assert_message!(p, Message::EndHand { payoffs, .. }, || {
                let payoff = &payoffs[0];
                assert_eq!(payoff.player_id, bb_player_id);

                // Winner wins blinds.
                assert_eq!(
                    payoff.chips,
                    table.state.big_blind + table.state.small_blind
                );
            });
        }
    }

    #[tokio::test]
    async fn multi_pots() {
        let mut table = TestTable::new(vec![500_000, 300_000, 100_000]);
        table.test_start_game().await;
        table.test_start_hand().await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // First player to act goes all in.
        let player = table.state.players.active_player().unwrap();
        let amount = player.chips + player.bet;
        table.bet(amount).await;

        // All players get a game update with the player action followed by an action
        // request for the next player to act.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Next player calls and goes all in.
        let player = table.state.players.active_player().unwrap();
        let amount = player.chips + player.bet;
        table.bet(amount).await;

        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });
            assert_message!(p, Message::ActionRequest { .. });
        }

        // Last player (BB) calls and goes all in.
        let player = table.state.players.active_player().unwrap();
        let amount = player.chips + player.bet;
        table.bet(amount).await;

        // All players went all in we should get the following messages.
        for p in table.players.iter_mut() {
            assert_message!(p, Message::GameUpdate { .. });

            // All players get a game update with the flop cards.
            assert_message!(p, Message::GameUpdate { .. });

            // All players get an update for the turn.
            assert_message!(p, Message::GameUpdate { .. });

            // And the river.
            assert_message!(p, Message::GameUpdate { .. });

            // Showdown message with all players cards.
            assert_message!(p, Message::GameUpdate { .. });

            // All players get a EndHand message with winner.
            assert_message!(p, Message::EndHand { payoffs, .. }, || {
                // We should have 3 payoffs, one player went all in with 100_000
                // another went all in for 300_000 and another for 500_000.

                // The one that went all in for 100_000 won the first pot for a total
                // of 300_000 so the other remaining players have 200_000 and 400_000
                // left. Of these remaining players the 200_000 won for a total of
                // 400_000 leaving the remaining player with 200_000 that will get
                // refundend.
                assert_eq!(payoffs.len(), 3);
                assert_eq!(payoffs[0].chips, Chips::new(300_000));
                assert_eq!(payoffs[1].chips, Chips::new(400_000));
                assert_eq!(payoffs[2].chips, Chips::new(200_000));
            });
        }
    }

    #[tokio::test]
    async fn split_win() {
        const JOIN_CHIPS: u32 = 100_000;

        let mut table = TestTable::new(vec![JOIN_CHIPS, JOIN_CHIPS, JOIN_CHIPS]);
        table.test_start_game().await;
        table.test_start_hand().await;

        // Set player cards so that we get a split win (another player has 7D 9H).
        let p = table.state.players.iter_mut().next().unwrap();
        p.hole_cards = PlayerCards::Cards(
            Card::new(Rank::Seven, Suit::Spades),
            Card::new(Rank::Nine, Suit::Diamonds),
        );

        // Preflop.
        table.bet(Chips::new(50_000)).await;
        table.call().await;
        table.call().await;
        table.drain_players_message();

        // Flop
        table.check().await;
        table.check().await;
        table.check().await;
        table.drain_players_message();

        // Turn
        table.check().await;
        table.check().await;
        table.check().await;
        table.drain_players_message();

        // River
        table.check().await;
        table.check().await;
        table.drain_players_message();

        table.check().await;

        for p in table.players.iter_mut() {
            // Update following last player check.
            assert_message!(p, Message::GameUpdate { .. });

            // Game update with showdown.
            assert_message!(p, Message::GameUpdate { .. });

            assert_message!(p, Message::EndHand { payoffs, .. }, || {
                // We should have 2 payoffs, with equal amount as two players have
                // the same cards value (7S, 9D) and (7D, 9H)
                assert_eq!(payoffs.len(), 2);
                assert_eq!(payoffs[0].chips, Chips::new(75_000));
                assert_eq!(payoffs[1].chips, Chips::new(75_000));
            });
        }
    }

    #[tokio::test]
    async fn blinds_increment() {
        let mut table = TestTable::new(vec![100_000, 100_000]);

        // First 4 hands blinds have initial value.
        (0..4).for_each(|_| table.state.update_blinds());
        assert_eq!(table.state.small_blind, State::START_GAME_SB);
        assert_eq!(table.state.big_blind, State::START_GAME_BB);

        // Next for hands blinds double.
        (0..4).for_each(|_| table.state.update_blinds());
        assert_eq!(table.state.small_blind, State::START_GAME_SB * 2);
        assert_eq!(table.state.big_blind, State::START_GAME_BB * 2);

        // Next 4 hands blinds double again.
        (0..4).for_each(|_| table.state.update_blinds());
        assert_eq!(table.state.small_blind, State::START_GAME_SB * 4);
        assert_eq!(table.state.big_blind, State::START_GAME_BB * 4);

        // Next 4 hands blinds double again.
        (0..4).for_each(|_| table.state.update_blinds());
        assert_eq!(table.state.small_blind, State::START_GAME_SB * 8);
        assert_eq!(table.state.big_blind, State::START_GAME_BB * 8);

        // After that we keep them at the same level
        (0..8).for_each(|_| table.state.update_blinds());
        assert_eq!(table.state.small_blind, State::START_GAME_SB * 12);
        assert_eq!(table.state.big_blind, State::START_GAME_BB * 12);

        // Test for overflow bug.
        (0..128).for_each(|_| table.state.update_blinds());
        assert_eq!(table.state.small_blind, State::START_GAME_SB * 12);
        assert_eq!(table.state.big_blind, State::START_GAME_BB * 12);
    }
}
