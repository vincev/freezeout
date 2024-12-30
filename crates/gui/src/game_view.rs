// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Game view.
use eframe::egui::*;
use log::{error, info};

use freezeout_core::{
    crypto::PeerId,
    message::{Message, SignedMessage},
    poker::{Chips, TableId},
};

use crate::{App, ConnectView, ConnectionEvent, View};

/// Connect view.
#[derive(Debug)]
pub struct GameView {
    connection_closed: bool,
    game_state: GameState,
}

/// Game player data.
#[derive(Debug)]
struct Player {
    player_id: PeerId,
    // Cache player id digits to avoid generation at every repaint.
    player_id_digits: String,
    nickname: String,
    chips: Chips,
}

/// The game state.
#[derive(Debug)]
struct GameState {
    table_id: TableId,
    players: Vec<Player>,
    error: Option<String>,
}

impl GameView {
    /// Creates a new [GameView].
    pub fn new(ctx: &Context) -> Self {
        ctx.request_repaint();

        Self {
            connection_closed: false,
            game_state: GameState {
                table_id: TableId::NO_TABLE,
                players: Vec::default(),
                error: None,
            },
        }
    }

    fn paint_table(&self, ui: &mut Ui, rect: &Rect) {
        fn paint_oval(ui: &mut Ui, rect: &Rect, fill: Color32) {
            let radius = rect.height() / 2.0;
            ui.painter().add(epaint::CircleShape {
                center: rect.left_center() + vec2(radius, 0.0),
                radius,
                fill,
                stroke: Stroke::NONE,
            });

            ui.painter().add(epaint::CircleShape {
                center: rect.right_center() - vec2(radius, 0.0),
                radius,
                fill,
                stroke: Stroke::NONE,
            });

            ui.painter().rect(
                Rect::from_center_size(
                    rect.center(),
                    vec2(rect.width() - 2.0 * radius, rect.height()),
                ),
                0.0,
                fill,
                Stroke::NONE,
            );
        }

        // Outer pad border
        paint_oval(ui, rect, Color32::from_rgb(200, 160, 80));

        // Table pad
        let mut outer = Color32::from_rgb(90, 90, 105);
        let inner = Color32::from_rgb(15, 15, 50);
        for pad in (2..45).step_by(3) {
            paint_oval(ui, &rect.shrink(pad as f32), outer);
            outer = outer.lerp_to_gamma(inner, 0.1);
        }

        // Inner pad border
        paint_oval(ui, &rect.shrink(50.0), Color32::from_rgb(200, 160, 80));

        // Outer table
        let mut outer = Color32::from_rgb(40, 110, 20);
        let inner = Color32::from_rgb(10, 140, 10);
        for pad in (52..162).step_by(5) {
            paint_oval(ui, &rect.shrink(pad as f32), outer);
            outer = outer.lerp_to_gamma(inner, 0.1);
        }

        // Cards board
        paint_oval(ui, &rect.shrink(162.0), Color32::from_gray(160));
        paint_oval(ui, &rect.shrink(164.0), inner);
    }

    fn paint_players(&self, ui: &mut Ui, rect: &Rect) {
        // Seats starting from mid bottom clock wise each point is a player center.
        let seats: &[Align2] = match self.game_state.players.len() {
            1 => &[Align2::CENTER_BOTTOM],
            2 => &[Align2::CENTER_BOTTOM, Align2::CENTER_TOP],
            3 => &[Align2::CENTER_BOTTOM, Align2::LEFT_TOP, Align2::RIGHT_TOP],
            4 => &[
                Align2::CENTER_BOTTOM,
                Align2::LEFT_CENTER,
                Align2::CENTER_TOP,
                Align2::RIGHT_CENTER,
            ],
            5 => &[
                Align2::CENTER_BOTTOM,
                Align2::LEFT_BOTTOM,
                Align2::LEFT_TOP,
                Align2::RIGHT_TOP,
                Align2::RIGHT_BOTTOM,
            ],
            _ => &[
                Align2::CENTER_BOTTOM,
                Align2::LEFT_BOTTOM,
                Align2::LEFT_TOP,
                Align2::CENTER_TOP,
                Align2::RIGHT_TOP,
                Align2::RIGHT_BOTTOM,
            ],
        };

        for (player, align) in self.game_state.players.iter().zip(seats) {
            player.paint(ui, rect, align);
        }
    }
}

impl View for GameView {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame, app: &mut App) {
        while let Some(event) = app.poll_network() {
            match event {
                ConnectionEvent::Open => {
                    self.connection_closed = false;
                }
                ConnectionEvent::Close => {
                    self.connection_closed = true;
                }
                ConnectionEvent::Error(e) => {
                    self.game_state.error = Some(format!("Connection error {e}"));
                    error!("Connection error {e}");
                }
                ConnectionEvent::Message(msg) => {
                    self.game_state.handle_message(msg, app);
                }
            }
        }

        Window::new("Freezeout Poker")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .title_bar(false)
            .frame(Frame::none().fill(Color32::from_gray(80)).rounding(7.0))
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(1024.0, 640.0), Sense::hover());
                let table_rect = Rect::from_center_size(rect.center(), rect.shrink(60.0).size());
                self.paint_table(ui, &table_rect);
                self.paint_players(ui, &rect);
            });
    }

    fn next(
        &mut self,
        _ctx: &Context,
        frame: &mut eframe::Frame,
        app: &mut App,
    ) -> Option<Box<dyn View>> {
        if self.connection_closed {
            Some(Box::new(ConnectView::new(frame.storage(), app)))
        } else {
            None
        }
    }
}

impl Player {
    fn paint(&self, ui: &mut Ui, rect: &Rect, align: &Align2) {
        const PLAYER_SIZE: Vec2 = vec2(120.0, 160.0);

        let rect = rect.shrink(20.0);
        let x = match align.x() {
            Align::LEFT => rect.left(),
            Align::Center => rect.center().x - PLAYER_SIZE.x / 2.0,
            Align::RIGHT => rect.right() - PLAYER_SIZE.x,
        };

        let y = match (align.x(), align.y()) {
            (Align::LEFT, Align::TOP) | (Align::RIGHT, Align::TOP) => {
                rect.top() + rect.height() / 4.0 - PLAYER_SIZE.y / 2.0
            }
            (Align::LEFT, Align::BOTTOM) | (Align::RIGHT, Align::BOTTOM) => {
                rect.bottom() - rect.height() / 4.0 - PLAYER_SIZE.y / 2.0
            }
            (Align::LEFT, Align::Center) | (Align::RIGHT, Align::Center) => {
                rect.bottom() - rect.height() / 2.0 - PLAYER_SIZE.y / 2.0
            }
            (Align::Center, Align::TOP) => rect.top(),
            (Align::Center, Align::BOTTOM) => rect.bottom() - PLAYER_SIZE.y,
            _ => unreachable!(),
        };

        let rect = Rect::from_min_size(pos2(x, y), PLAYER_SIZE);
        let id_rect = self.paint_id(ui, &rect, align);
        self.paint_name_and_chips(ui, &id_rect);
    }

    fn paint_id(&self, ui: &mut Ui, rect: &Rect, align: &Align2) -> Rect {
        let rect = rect.shrink(10.0);

        let layout_job = text::LayoutJob {
            wrap: text::TextWrapping::wrap_at_width(75.0),
            ..text::LayoutJob::single_section(
                self.player_id_digits.clone(),
                TextFormat {
                    font_id: FontId::new(13.0, FontFamily::Monospace),
                    extra_letter_spacing: 1.0,
                    color: Color32::from_rgb(20, 180, 20),
                    ..Default::default()
                },
            )
        };

        let galley = ui.painter().layout_job(layout_job);

        let min_pos = if let Align::RIGHT = align.x() {
            rect.right_top() - vec2(galley.size().x, 0.0)
        } else {
            rect.left_top()
        };

        // Paint peer id rect.
        let rect = Rect::from_min_size(min_pos, galley.rect.size());

        let bg_rect = rect.expand(5.0);
        paint_border(ui, &bg_rect);

        let text_pos = rect.left_top();
        ui.painter().galley(text_pos, galley, Color32::DARK_GRAY);

        bg_rect
    }

    fn paint_name_and_chips(&self, ui: &mut Ui, rect: &Rect) {
        let bg_rect = Rect::from_min_size(
            rect.left_bottom() + vec2(0.0, 10.0),
            vec2(rect.width(), 40.0),
        );

        paint_border(ui, &bg_rect);

        let painter = ui.painter().with_clip_rect(bg_rect.shrink(3.0));

        let text_color = Color32::from_rgb(20, 180, 20);
        let font = FontId::new(13.0, FontFamily::Monospace);

        let galley =
            ui.painter()
                .layout_no_wrap(self.nickname.to_string(), font.clone(), text_color);

        painter.galley(
            bg_rect.left_top() + vec2(5.0, 4.0),
            galley.clone(),
            text_color,
        );

        let chips_pos = bg_rect.left_top() + vec2(0.0, galley.size().y);

        let galley = ui
            .painter()
            .layout_no_wrap(self.chips.to_string(), font, text_color);

        painter.galley(chips_pos + vec2(5.0, 7.0), galley.clone(), text_color);
    }
}

fn paint_border(ui: &mut Ui, rect: &Rect) {
    let border_color = Color32::from_gray(20);
    ui.painter().rect(*rect, 5.0, border_color, Stroke::NONE);

    for (idx, &color) in (0..6).zip(&[100, 120, 140, 100, 80]) {
        let border_rect = rect.expand(idx as f32);
        let stroke = Stroke::new(1.0, Color32::from_gray(color as u8));
        ui.painter().rect_stroke(border_rect, 5.0, stroke);
    }
}

impl GameState {
    fn handle_message(&mut self, msg: Box<SignedMessage>, app: &mut App) {
        match msg.to_message() {
            Message::TableJoined { table_id, chips } => {
                self.table_id = table_id;
                // Add this player as the first player in the players list.
                let player_id = app.player_id().clone();
                self.players.push(Player {
                    player_id_digits: player_id.digits(),
                    player_id,
                    nickname: app.nickname().to_string(),
                    chips,
                });

                info!(
                    "Joined table {} {:?}",
                    table_id,
                    self.players.last().unwrap()
                )
            }
            Message::PlayerJoined {
                player_id,
                nickname,
                chips,
            } => {
                self.players.push(Player {
                    player_id_digits: player_id.digits(),
                    player_id,
                    nickname,
                    chips,
                });

                info!("Added player {:?}", self.players.last().unwrap())
            }
            Message::PlayerLeft(player_id) => {
                self.players.retain(|p| p.player_id != player_id);
            }
            Message::Error(e) => self.error = Some(e),
            _ => {}
        }
    }
}
