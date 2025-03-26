// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Bot.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;
use rand::prelude::*;

use freezeout_core::{
    game_state::{ActionRequest, GameState},
    message::PlayerAction,
    poker::{Chips, PlayerCards},
};

use freezeout_bot::Strategy;

#[derive(Clone)]
struct AlwaysCallOrCheck;

impl Strategy for AlwaysCallOrCheck {
    fn execute(&mut self, req: &ActionRequest, state: &GameState) -> (PlayerAction, Chips) {
        // Some randomness.
        let p = random::<f64>();

        // Get local player.
        let player = &state.players()[0];
        if let PlayerCards::Cards(c1, c2) = player.cards {
            // Raise preflop with a pair.
            if c1.rank() == c2.rank()
                && state.board().is_empty()
                && req.can_raise()
                && matches!(player.action, PlayerAction::None)
                && matches!(player.action, PlayerAction::BigBlind)
                && matches!(player.action, PlayerAction::SmallBlind)
                && p > 0.2
            {
                return (PlayerAction::Raise, req.min_raise);
            }
        }

        if p < 0.1 && !req.can_check() {
            (PlayerAction::Fold, Chips::ZERO)
        } else if req.can_call() {
            (PlayerAction::Call, Chips::ZERO)
        } else if req.can_check() {
            (PlayerAction::Check, Chips::ZERO)
        } else {
            (PlayerAction::Fold, Chips::ZERO)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    freezeout_bot::run(|| AlwaysCallOrCheck).await
}
