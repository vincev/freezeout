// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Connection dialog view.
use eframe::egui::*;

use freezeout_core::{game_state::GameState, message::Message, poker::Chips};

use crate::{App, ConnectView, ConnectionEvent, GameView, View};

const TEXT_FONT: FontId = FontId::new(16.0, FontFamily::Monospace);

/// Connect view.
pub struct AccountView {
    player_id: String,
    nickname: String,
    game_state: GameState,
    chips: Chips,
    error: String,
    connection_closed: bool,
    table_joined: bool,
    message: String,
}

impl AccountView {
    /// Creates a new connect view.
    pub fn new(chips: Chips, app: &App) -> Self {
        Self {
            player_id: app.player_id().digits(),
            nickname: app.nickname().to_string(),
            game_state: GameState::new(app.player_id().clone(), app.nickname().to_string()),
            chips,
            error: String::default(),
            connection_closed: false,
            table_joined: false,
            message: String::default(),
        }
    }
}

impl View for AccountView {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame, app: &mut App) {
        while let Some(event) = app.poll_network() {
            match event {
                ConnectionEvent::Open => {}
                ConnectionEvent::Close => {
                    self.error = "Connection closed".to_string();
                    self.connection_closed = true;
                }
                ConnectionEvent::Error(e) => {
                    self.error = format!("Connection error {e}");
                }
                ConnectionEvent::Message(msg) => {
                    match msg.message() {
                        Message::TableJoined { .. } => {
                            self.table_joined = true;
                        }
                        Message::NotEnoughChips => {
                            self.message = "Not enough chips to play, reconnect later".to_string();
                        }
                        Message::NoTablesLeft => {
                            self.message = "All tables are busy, reconnect later".to_string();
                        }
                        _ => {}
                    }

                    self.game_state.handle_message(msg);
                }
            }
        }

        Window::new("Account")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 150.0))
            .max_width(400.0)
            .show(ctx, |ui| {
                ui.group(|ui| {
                    Grid::new("my_grid")
                        .num_columns(2)
                        .spacing([40.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Nickname").font(TEXT_FONT));
                            ui.label(RichText::new(&self.nickname).font(TEXT_FONT));
                            ui.end_row();

                            ui.label(RichText::new("Player ID").font(TEXT_FONT));
                            ui.label(RichText::new(&self.player_id).font(TEXT_FONT));
                            ui.end_row();

                            ui.label(RichText::new("Chips").font(TEXT_FONT));
                            ui.label(RichText::new(self.chips.to_string()).font(TEXT_FONT));
                            ui.end_row();
                        });
                });

                ui.add_space(10.0);

                ui.vertical_centered(|ui| {
                    if !self.message.is_empty() {
                        ui.label(
                            RichText::new(&self.message)
                                .font(TEXT_FONT)
                                .color(Color32::RED),
                        );

                        ui.add_space(10.0);
                    }

                    let btn = Button::new(RichText::new("Join Table").font(TEXT_FONT));
                    if ui.add_sized(vec2(180.0, 30.0), btn).clicked() {
                        app.send_message(Message::JoinTable);
                    };
                });
            });
    }

    fn next(
        &mut self,
        ctx: &Context,
        frame: &mut eframe::Frame,
        app: &mut App,
    ) -> Option<Box<dyn View>> {
        if self.connection_closed {
            Some(Box::new(ConnectView::new(frame.storage(), app)))
        } else if self.table_joined {
            let empty_state = GameState::new(app.player_id().clone(), app.nickname().to_string());
            Some(Box::new(GameView::new(
                ctx,
                std::mem::replace(&mut self.game_state, empty_state),
            )))
        } else {
            None
        }
    }
}
