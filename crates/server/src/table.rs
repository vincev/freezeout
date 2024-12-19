// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Table state types.
use anyhow::{bail, Result};
use log::info;
use parking_lot::Mutex;

use freezeout_core::{crypto::PlayerId, message::Message};

/// Table state shared by all players who joined the table.
#[derive(Debug)]
pub struct Table {
    state: Mutex<State>,
}

/// Internal table state.
#[derive(Debug)]
struct State {
    seats: usize,
    players: Vec<Player>,
}

/// A table player state.
#[derive(Debug)]
struct Player {
    player_id: PlayerId,
    nickname: String,
}

impl Table {
    /// Creates a new table with a number of seats.
    pub fn new(seats: usize) -> Self {
        Self {
            state: Mutex::new(State {
                seats,
                players: Vec::with_capacity(seats),
            }),
        }
    }

    /// Checks if this table is full.
    pub fn is_full(&self) -> bool {
        let state = self.state.lock();
        state.players.len() == state.seats
    }

    /// Returns the number of players at the table.
    pub fn count_joined(&self) -> usize {
        let state = self.state.lock();
        state.players.len()
    }

    /// A player joins this table.
    ///
    /// Returns error if the table is full or the player has already joined.
    pub fn join(&self, player_id: &PlayerId, nickname: &str) -> Result<()> {
        let mut state = self.state.lock();

        if state.players.len() == state.seats {
            bail!("Table full");
        }

        if state.players.iter().any(|p| &p.player_id == player_id) {
            bail!("Player has already joined");
        }

        state.players.push(Player {
            player_id: player_id.clone(),
            nickname: nickname.to_string(),
        });

        info!("Player {player_id} joined the table.");

        Ok(())
    }

    /// A player leaves the table.
    pub fn leave(&self, player_id: &PlayerId) {
        let mut state = self.state.lock();
        state.players.retain(|p| &p.player_id != player_id);

        info!("Player {player_id} left the table.");
    }

    /// Handle a message from a player.
    pub fn handle_message(&self, sender: &PlayerId, msg: Message) {
        info!("Player {sender} message: {msg:?}");
    }
}
