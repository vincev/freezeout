// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Bot.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;

use freezeout_core::{
    game_state::{ActionRequest, GameState},
    message::PlayerAction,
    poker::Chips,
};

use freezeout_bot::Strategy;

#[derive(Clone)]
struct AlwaysCallOrCheck;

impl Strategy for AlwaysCallOrCheck {
    fn execute(&mut self, req: &ActionRequest, _state: &GameState) -> (PlayerAction, Chips) {
        if req.actions.iter().any(|a| matches!(a, PlayerAction::Call)) {
            (PlayerAction::Call, Chips::ZERO)
        } else if req.actions.iter().any(|a| matches!(a, PlayerAction::Check)) {
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
