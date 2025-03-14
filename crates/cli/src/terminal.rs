// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Terminal I/O.
use anyhow::Result;
use crossterm::{
    cursor,
    event::{Event, EventStream, KeyCode, KeyEvent},
    execute, queue, style,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use std::io;

use freezeout_core::{
    game_state::{GameState, Player},
    message::{Message, PlayerAction},
    poker::{Card, Chips, PlayerCards},
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

        let mut view = View {
            state,
            betting: None,
        };

        // Start the game.
        view.start_game(net).await?;
    } else {
        println!("No tables available, try later");
    }

    Ok(())
}

struct View {
    state: GameState,
    betting: Option<BetParams>,
}

struct BetParams {
    min_raise: u32,
    big_blind: u32,
    raise_value: u32,
}

impl View {
    async fn start_game(&mut self, mut net: Network) -> Result<()> {
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        execute!(stdout, cursor::Hide)?;

        self.print_game_state(&mut stdout)?;

        let mut reader = EventStream::new();
        loop {
            tokio::select! {
                // We have received a message from the client.
                res = net.recv() => {
                    let msg = res?;
                    if let Message::ShowAccount { .. } = msg.message() {
                        break;
                    }

                    self.state.handle_message(msg);
                    self.print_game_state(&mut stdout)?;
                },
                // We have received an event form the terminal.
                res = reader.next() => {
                    if let Some(Ok(Event::Key(KeyEvent { code, .. }))) = res {
                        if code == KeyCode::Char('q') {
                            break;
                        }

                        self.handle_action(code, &mut net).await?;
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

    async fn handle_action(&mut self, code: KeyCode, net: &mut Network) -> Result<()> {
        if let Some(req) = self.state.action_request() {
            match code {
                // Fold
                KeyCode::Char('f') => {
                    net.send(Message::ActionResponse {
                        action: PlayerAction::Fold,
                        amount: Chips::ZERO,
                    })
                    .await?;
                    self.state.reset_action_request();
                }
                // Call or check
                KeyCode::Char('c') => {
                    let action = req
                        .actions
                        .iter()
                        .find(|a| matches!(a, PlayerAction::Call | PlayerAction::Check));
                    if let Some(&action) = action {
                        net.send(Message::ActionResponse {
                            action,
                            amount: Chips::ZERO,
                        })
                        .await?;
                        self.state.reset_action_request();
                    }
                }
                // Bet
                KeyCode::Char('b') => {
                    if req.actions.iter().any(|a| matches!(a, PlayerAction::Bet))
                        && self.betting.is_none()
                    {
                        self.betting = Some(BetParams {
                            min_raise: req.min_raise.into(),
                            big_blind: req.big_blind.into(),
                            raise_value: req.min_raise.into(),
                        });
                    }
                }
                // Raise
                KeyCode::Char('r') => {
                    if req.actions.iter().any(|a| matches!(a, PlayerAction::Raise))
                        && self.betting.is_none()
                    {
                        self.betting = Some(BetParams {
                            min_raise: req.min_raise.into(),
                            big_blind: req.big_blind.into(),
                            raise_value: req.min_raise.into(),
                        });
                    }
                }
                // Bet more
                KeyCode::Up => {
                    if let Some(p) = self.betting.as_mut() {
                        let max_bet = self
                            .state
                            .players()
                            .first()
                            .map(|p| (p.chips + p.bet).into())
                            .unwrap();
                        p.raise_value = (p.raise_value + p.big_blind).min(max_bet);
                    }
                }
                // Bet less
                KeyCode::Down => {
                    if let Some(p) = self.betting.as_mut() {
                        p.raise_value = (p.raise_value - p.big_blind).max(p.min_raise);
                    }
                }
                // Confirm betting
                KeyCode::Enter => {
                    if let Some(p) = &self.betting {
                        let action = req
                            .actions
                            .iter()
                            .find(|a| matches!(a, PlayerAction::Bet | PlayerAction::Raise));
                        if let Some(&action) = action {
                            net.send(Message::ActionResponse {
                                action,
                                amount: Chips::new(p.raise_value),
                            })
                            .await?;
                            self.state.reset_action_request();
                            self.betting = None;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn print_game_state(&mut self, w: &mut impl io::Write) -> Result<()> {
        execute!(w, Clear(ClearType::All))?;

        let mut row = 0;

        // Print the board and the pot
        print_board(w, self.state.board(), self.state.pot(), row)?;
        row += 1;

        // Print remote players, skip the first player as it is the local player.
        for player in self.state.players().iter().skip(1) {
            print_player(w, player, row)?;
            row += 1;
        }

        // Print the local player.
        for player in self.state.players().iter().take(1) {
            print_player(w, player, row)?;
            row += 1;
        }

        // Print control for local player.
        self.print_controls(w, row)?;

        w.flush()?;

        Ok(())
    }

    fn print_controls(&mut self, w: &mut impl io::Write, row: u16) -> Result<()> {
        if let Some(req) = self.state.action_request() {
            queue!(
                w,
                cursor::MoveTo(0, row),
                style::SetBackgroundColor(style::Color::Black),
                style::SetForegroundColor(style::Color::DarkGreen),
                style::Print("Action    |")
            )?;

            // Print buttons.
            for action in &req.actions {
                let label = format!("{:^10.10}", action.label());
                queue!(
                    w,
                    style::SetBackgroundColor(style::Color::DarkGreen),
                    style::SetForegroundColor(style::Color::Black),
                    style::Print(label),
                    style::SetBackgroundColor(style::Color::Black),
                    style::SetForegroundColor(style::Color::DarkGreen),
                    style::Print(" "),
                )?;
            }

            if let Some(params) = &self.betting {
                let amount = format!("{:^10.10}", Chips::new(params.raise_value).to_string());
                queue!(w, style::Print(amount),)?;
            }
        }
        Ok(())
    }
}

fn print_player(w: &mut impl io::Write, p: &Player, row: u16) -> Result<()> {
    // Move cursor to the beginning of the row.
    queue!(w, cursor::MoveTo(0, row))?;

    // Print id or timer with inverted colors.
    let (id, bg, fg) = if let Some(timer) = p.action_timer {
        (
            format!("{timer:02}"),
            style::Color::DarkGreen,
            style::Color::Black,
        )
    } else {
        (
            p.player_id_digits[0..10].to_string(),
            style::Color::Black,
            style::Color::DarkGreen,
        )
    };

    queue!(
        w,
        style::SetBackgroundColor(bg),
        style::SetForegroundColor(fg),
        style::Print(format!("{id:^10.10}")),
    )?;

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
        "|{:<10.10}|{:<10.10}|{:<10.10}|{:<10.10}|{:<6}",
        p.nickname,
        p.chips.to_string(),
        action,
        bet,
        cards
    );

    queue!(
        w,
        style::SetBackgroundColor(style::Color::Black),
        style::SetForegroundColor(style::Color::DarkGreen),
        style::Print(text)
    )?;

    Ok(())
}

fn print_board(w: &mut impl io::Write, board: &[Card], pot: Chips, row: u16) -> Result<()> {
    // Move cursor to the beginning of the row.
    queue!(w, cursor::MoveTo(0, row))?;

    let cards = board
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    let pot = if pot > Chips::ZERO {
        pot.to_string()
    } else {
        String::default()
    };

    let text = format!("Board     |{cards:<21.21}|Pot       |{pot:<10.10}|");
    queue!(
        w,
        style::SetBackgroundColor(style::Color::Black),
        style::SetForegroundColor(style::Color::DarkGreen),
        style::Print(text)
    )?;

    Ok(())
}
