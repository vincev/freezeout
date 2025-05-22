// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! A simple example bot strategy.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
use anyhow::Result;
use clap::Parser;

use freezeout_bot::{
    Strategy,
    core::{
        game_state::{ActionRequest, GameState},
        message::PlayerAction,
        poker::{Chips, PlayerCards},
    },
};

#[derive(Clone)]
struct AlwaysCallOrCheck;

impl Strategy for AlwaysCallOrCheck {
    fn execute(&mut self, req: &ActionRequest, state: &GameState) -> (PlayerAction, Chips) {
        // Some randomness.
        let p = rand::random::<f64>();

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

#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
struct Cli {
    /// Number of clients to run.
    #[clap(long, short, value_parser = clap::value_parser!(u8).range(1..=5))]
    clients: u8,
    /// The server WebSocker url (eg. ws://127.0.0.1:9871).
    #[clap(long, short, default_value = "ws://127.0.0.1:9871")]
    url: String,
    /// Help long flag.
    #[clap(long, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = freezeout_bot::Config {
        clients: cli.clients,
        url: cli.url,
    };

    freezeout_bot::run(config, || AlwaysCallOrCheck).await
}
