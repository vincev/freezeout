// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Terminal I/O.
use anyhow::Result;
use crossterm::{
    cursor,
    event::{Event, EventStream, KeyCode},
    execute, queue,
    style::{self, Stylize},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use std::io;

use freezeout_core::{
    game_state::{GameState, Player},
    message::{Message, PlayerAction},
    poker::{Chips, PlayerCards},
};

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
        start_game(net, &mut state).await?;
    } else {
        println!("No tables available, try later");
    }

    Ok(())
}

async fn start_game(mut net: Network, state: &mut GameState) -> Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, cursor::Hide)?;

    print_players(&mut stdout, state)?;

    let mut reader = EventStream::new();
    loop {
        tokio::select! {
            // We have received a message from the client.
            res = net.recv() => {
                let msg = res?;
                if let Message::ShowAccount { .. } = msg.message() {
                    break;
                }

                state.handle_message(msg);
                print_players(&mut stdout, state)?;
            },
            // We have received an event form the terminal.
            res = reader.next() => {
                if let Some(Ok(event)) = res {
                    if event == Event::Key(KeyCode::Char('q').into()) {
                        break;
                    }
                }
            },
        };
    }

    execute!(
        stdout,
        Clear(ClearType::All),
        cursor::MoveTo(0, 0),
        cursor::Show
    )?;
    disable_raw_mode()?;

    Ok(())
}

fn print_players(w: &mut impl io::Write, state: &mut GameState) -> Result<()> {
    execute!(w, Clear(ClearType::All))?;

    // Rows for each player, the local player that is the first in the players array
    // appears at the bottom.
    let rows: &[u16] = match state.players().len() {
        1 => &[1],
        2 => &[2, 1],
        3 => &[3, 1, 2],
        4 => &[4, 1, 2, 3],
        5 => &[5, 1, 2, 3, 4],
        _ => &[6, 1, 2, 3, 4, 5],
    };

    for (player, row) in state.players().iter().zip(rows) {
        print_player(w, player, *row)?;
    }

    w.flush()?;

    Ok(())
}

fn print_player(w: &mut impl io::Write, p: &Player, row: u16) -> Result<()> {
    // Print the first 10 digits of the player identity or timer
    let id = if let Some(timer) = p.action_timer {
        format!("  00:{timer:02}")
    } else {
        p.player_id_digits[0..10].to_string()
    };

    let action = if !matches!(p.action, PlayerAction::None) || p.winning_chips > Chips::ZERO {
        if p.winning_chips > Chips::ZERO {
            "WINNER"
        } else {
            p.action.label()
        }
    } else {
        ""
    };

    let bet = if p.bet > Chips::ZERO || p.winning_chips > Chips::ZERO {
        if p.bet > Chips::ZERO {
            p.bet.to_string()
        } else {
            p.winning_chips.to_string()
        }
    } else {
        "".to_string()
    };

    let cards = match p.cards {
        PlayerCards::None => "".to_string(),
        PlayerCards::Covered => "▒▒ ▒▒".to_string(),
        PlayerCards::Cards(c1, c2) => format!("{} {}", c1, c2),
    };

    let text = format!(
        "{id:^10.10}|{:<10.10}|{:<10.10}|{:<10.10}|{:<10.10}|{:<6}",
        p.nickname,
        p.chips.to_string(),
        action,
        bet,
        cards
    );

    queue!(
        w,
        cursor::MoveTo(0, row),
        style::PrintStyledContent(text.as_str().dark_green())
    )?;

    Ok(())
}
