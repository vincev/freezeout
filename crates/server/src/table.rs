// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table state types.
use ahash::{AHashMap, AHashSet};
use anyhow::{bail, Result};
use log::{error, info};
use rand::seq::SliceRandom;
use std::{
    cmp::Ordering,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time,
};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::{Message, PlayerAction, PlayerUpdate, SignedMessage},
    poker::{Card, Chips, Deck, HandValue, PlayerCards, TableId},
};

use crate::db::Db;

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
    /// The receiver left the table.
    PlayerLeft,
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

        let mut task = TableTask {
            table_id: TableId::new_id(),
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

        Self { commands_tx }
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
    /// The player action timer.
    action_timer: Option<Instant>,
    /// This player cards that are visible to all other players.
    public_cards: PlayerCards,
    /// This player private cards.
    hole_cards: PlayerCards,
    /// This player is active in the hand.
    is_active: bool,
    /// The player has the button.
    has_button: bool,
}

impl Player {
    fn new(
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
    async fn send(&self, msg: SignedMessage) {
        let _ = self.table_tx.send(TableMessage::Send(msg)).await;
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

    /// Sets this player in fold state.
    fn fold(&mut self) {
        self.is_active = false;
        self.action = PlayerAction::Fold;
        self.hole_cards = PlayerCards::None;
        self.public_cards = PlayerCards::None;
        self.action_timer = None;
    }
}

/// The table players state.
#[derive(Debug, Default)]
struct PlayersState {
    players: Vec<Player>,
    active_player: Option<usize>,
}

impl PlayersState {
    /// Adds a player to the table.
    fn join(&mut self, player: Player) {
        self.players.push(player);
    }

    /// Remove all players.
    fn clear(&mut self) {
        self.players.clear();
        self.active_player = None;
    }

    /// Removes a player from the table.
    fn leave(&mut self, player_id: &PeerId) -> Option<Player> {
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
    fn shuffle_seats(&mut self) {
        let mut rng = rand::thread_rng();
        self.players.shuffle(&mut rng);
    }

    /// Returns total number of players.
    fn count(&self) -> usize {
        self.players.len()
    }

    /// Returns the number of active players.
    fn count_active(&self) -> usize {
        self.players.iter().filter(|p| p.is_active).count()
    }

    /// Returns the number of player who have chips.
    fn count_with_chips(&self) -> usize {
        self.players
            .iter()
            .filter(|p| p.chips > Chips::ZERO)
            .count()
    }

    /// Returns the active player.
    fn active_player(&mut self) -> Option<&mut Player> {
        self.active_player
            .and_then(|idx| self.players.get_mut(idx))
            .filter(|p| p.is_active)
    }

    /// Check if this player is active.
    fn is_active(&self, player_id: &PeerId) -> bool {
        self.active_player
            .and_then(|idx| self.players.get(idx))
            .map(|p| &p.player_id == player_id)
            .unwrap_or(false)
    }

    /// Returns an iterator to all players.
    fn iter(&self) -> impl Iterator<Item = &Player> {
        self.players.iter()
    }

    /// Returns a mutable iterator to all players.
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.iter_mut()
    }

    /// Activate the next player if there is more than one active player.
    fn next_player(&mut self) {
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
    fn start_hand(&mut self) {
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
    fn start_round(&mut self) {
        self.active_player = None;

        for (idx, p) in self.players.iter().enumerate() {
            if p.chips > Chips::ZERO && p.is_active {
                self.active_player = Some(idx);
                return;
            }
        }
    }

    /// The hand has ended disable any active player.
    fn end_hand(&mut self) {
        self.active_player = None;
        self.players.iter_mut().for_each(Player::end_hand);
    }
}

/// A pot that contains players bets.
#[derive(Debug, Default)]
struct Pot {
    players: AHashSet<PeerId>,
    chips: Chips,
}

/// Internal table state.
#[derive(Debug)]
struct State {
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
}

impl State {
    const ACTION_TIMEOUT: Duration = Duration::from_secs(15);

    /// Create a new state.
    fn new(table_id: TableId, seats: usize, sk: Arc<SigningKey>, db: Db) -> Self {
        Self {
            table_id,
            seats,
            sk,
            db,
            hand_state: HandState::WaitForPlayers,
            small_blind: 10_000.into(),
            big_blind: 20_000.into(),
            players: PlayersState::default(),
            deck: Deck::new_and_shuffled(),
            last_bet: Chips::ZERO,
            min_raise: Chips::ZERO,
            pots: vec![Pot::default()],
            board: Vec::default(),
            new_hand_start_time: None,
        }
    }

    /// A player tries to join the table.
    async fn join(
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
    async fn leave(&mut self, player_id: &PeerId) {
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
    async fn message(&mut self, msg: SignedMessage) {
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

    async fn tick(&mut self) {
        if let Some(dt) = self.new_hand_start_time {
            if dt.elapsed() > Duration::from_secs(5) {
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
        self.players.next_player();

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
        self.players.shuffle_seats();

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

        self.players.next_player();

        if let Some(player) = self.players.active_player() {
            player.bet(PlayerAction::BigBlind, self.big_blind);
        };

        self.last_bet = self.big_blind;
        self.min_raise = self.big_blind;

        // Create a new deck.
        self.deck = Deck::new_and_shuffled();

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
                player.hole_cards = PlayerCards::Cards(self.deck.deal(), self.deck.deal());
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

        self.players.next_player();
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

        self.broadcast_game_update().await;
        self.enter_end_hand().await;
    }

    async fn enter_end_hand(&mut self) {
        self.hand_state = HandState::EndHand;

        self.update_pots();
        let winners = self.pay_bets();

        // Update players and broadcast update to all players.
        self.players.end_hand();
        self.broadcast_game_update().await;
        self.broadcast(Message::EndHand { winners }).await;

        // End game or set timer to start new hand.
        if self.players.count_with_chips() < 2 {
            self.enter_end_game().await;
        } else {
            self.new_hand_start_time = Some(Instant::now());
        }
    }

    async fn enter_end_game(&mut self) {
        self.hand_state = HandState::EndGame;

        // Wait some time to give time to player to see the end game.
        time::sleep(Duration::from_secs(5)).await;

        // Pay the players and tell them the game has finished and leave the table.
        for player in self.players.iter() {
            let _ = player.table_tx.send(TableMessage::PlayerLeft).await;
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

    fn pay_bets(&mut self) -> Vec<(PeerId, Chips)> {
        let mut winners = AHashMap::new();

        match self.players.count_active() {
            1 => {
                // If one player left gets all the chips.
                if let Some(player) = self.players.active_player() {
                    for pot in self.pots.drain(..) {
                        player.chips += pot.chips;
                        *winners.entry(player.player_id.clone()).or_default() += pot.chips;
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

                    if let Some((p, _hv)) = winner {
                        p.chips += pot.chips;
                        *winners.entry(p.player_id.clone()).or_default() += pot.chips;
                    }
                }
            }
            _ => {}
        }

        winners.into_iter().collect()
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
        if self.players.count_with_chips() < 2 {
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
