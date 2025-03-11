// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Terminal I/O.
use anyhow::Result;

use freezeout_core::{game_state::GameState, message::Message};

use crate::network::Network;

/// Runs the terminal loop.
pub async fn run(mut net: Network, nickname: String) -> Result<()> {
    // Try to join a table.
    net.send(Message::JoinTable).await?;

    let msg = net.recv().await?;
    if let Message::TableJoined { .. } = msg.message() {
        // We join a table, create a GameState and start the game.
        let mut state = GameState::new(net.player_id(), nickname);
        // Update the state with the table details.
        state.handle_message(msg);
        // Start the game.
        start_game(net, state).await?;
    } else {
        println!("No tables available, try later");
    }

    Ok(())
}

async fn start_game(mut net: Network, mut state: GameState) -> Result<()> {
    loop {
        let msg = net.recv().await?;
        // The hand has ended.
        if let Message::ShowAccount { .. } = msg.message() {
            break;
        }

        state.handle_message(msg);
        for player in state.players() {
            println!("{player:?}");
        }
    }

    Ok(())
}
