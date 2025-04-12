// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Game view.
use eframe::egui::*;
use log::error;

use freezeout_core::{
    game_state::{GameState, Player},
    message::{Message, PlayerAction},
    poker::{Chips, PlayerCards},
};

use crate::{AccountView, App, ConnectView, ConnectionEvent, Textures, View};

/// Connect view.
pub struct GameView {
    connection_closed: bool,
    game_state: GameState,
    error: Option<String>,
    bet_params: Option<BetParams>,
    show_account: Option<Chips>,
    show_legend: bool,
}

struct BetParams {
    min_raise: u32,
    big_blind: u32,
    raise_value: u32,
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
                    self.error = Some(format!("Connection error {e}"));
                    error!("Connection error {e}");

                    app.close_connection();
                    self.connection_closed = true;
                }
                ConnectionEvent::Message(msg) => {
                    if let Message::ShowAccount { chips } = msg.message() {
                        self.show_account = Some(*chips);
                    }

                    if let Message::StartHand = msg.message() {
                        self.bet_params = None;
                    }

                    self.game_state.handle_message(msg);
                }
            }
        }

        Window::new("Freezeout Poker")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .title_bar(false)
            .frame(Frame::NONE.fill(Color32::from_gray(80)).corner_radius(7.0))
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(1024.0, 640.0), Sense::hover());
                let table_rect = Rect::from_center_size(rect.center(), rect.shrink(60.0).size());
                self.paint_table(ui, &table_rect);
                self.paint_board(ui, &table_rect, app);
                self.paint_pot(ui, &table_rect);
                self.paint_players(ui, &rect, app);
                self.paint_close_button(ui, &rect, app);
                self.paint_help_button(ui, &rect);
                self.paint_server_key(ui, &rect);
                self.paint_legend(ui, &rect);
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
        } else if let Some(chips) = self.show_account {
            Some(Box::new(AccountView::new(chips, app)))
        } else {
            None
        }
    }
}

impl GameView {
    const TEXT_COLOR: Color32 = Color32::from_rgb(20, 150, 20);
    const TEXT_FONT: FontId = FontId::new(15.0, FontFamily::Monospace);
    const BG_COLOR: Color32 = Color32::from_gray(20);
    const ACTION_BUTTON_LX: f32 = 81.0;
    const ACTION_BUTTON_LY: f32 = 35.0;
    const SMALL_BUTTON_SZ: Vec2 = vec2(30.0, 30.0);

    /// Creates a new [GameView].
    pub fn new(ctx: &Context, game_state: GameState) -> Self {
        ctx.request_repaint();

        Self {
            connection_closed: false,
            game_state,
            error: None,
            bet_params: None,
            show_account: None,
            show_legend: false,
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
                StrokeKind::Inside,
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

        if !self.game_state.game_started() {
            let players = self.game_state.players().len();
            let seats = self.game_state.seats();
            let missing = seats.saturating_sub(players);

            let msg = if missing > 1 {
                format!("Waiting for {missing} players to join")
            } else {
                "Waiting for 1 player to join".to_string()
            };

            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::new(30.0, FontFamily::Monospace),
                Color32::from_gray(180),
            );
        }
    }

    fn paint_board(&self, ui: &mut Ui, rect: &Rect, app: &App) {
        const CARD_SIZE: Vec2 = vec2(38.0, 72.0);
        const BORDER: f32 = 5.0;

        if self.game_state.board().is_empty() {
            return;
        }

        let mut card_rect = Rect::from_min_size(
            rect.center() - vec2(CARD_SIZE.x * 2.5 + 2.0 * BORDER, CARD_SIZE.y / 2.0 + 20.0),
            CARD_SIZE,
        );

        for card in self.game_state.board() {
            let tx = app.textures.card(*card);
            Image::new(&tx).corner_radius(5.0).paint_at(ui, card_rect);

            card_rect = card_rect.translate(vec2(CARD_SIZE.x + BORDER, 0.0));
        }
    }

    fn paint_pot(&self, ui: &mut Ui, rect: &Rect) {
        const POT_SIZE: Vec2 = vec2(120.0, 40.0);

        if self.game_state.pot() > Chips::ZERO {
            let rect = Rect::from_min_size(
                rect.center() - vec2(POT_SIZE.x / 2.0, -POT_SIZE.y),
                POT_SIZE,
            );

            paint_border(ui, &rect);

            let galley = ui.painter().layout_no_wrap(
                self.game_state.pot().to_string(),
                FontId::new(18.0, FontFamily::Monospace),
                Self::TEXT_COLOR,
            );

            let text_offset = (rect.size() - galley.rect.size()) / 2.0;

            ui.painter()
                .galley(rect.left_top() + text_offset, galley, Self::TEXT_COLOR);
        }
    }

    fn paint_players(&mut self, ui: &mut Ui, rect: &Rect, app: &mut App) {
        // Seats starting from mid bottom clock wise each point is a player center.
        let seats: &[Align2] = match self.game_state.players().len() {
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

        for (player, align) in self.game_state.players().iter().zip(seats) {
            self.paint_player(player, ui, rect, align, app);
        }

        self.paint_action_controls(ui, rect, app);
    }

    fn paint_player(
        &self,
        player: &Player,
        ui: &mut Ui,
        rect: &Rect,
        align: &Align2,
        app: &mut App,
    ) {
        let rect = player_rect(rect, align);
        let id_rect = self.paint_player_id(player, ui, &rect, align);
        self.paint_player_name_and_chips(player, ui, &id_rect);
        self.paint_player_cards(player, ui, &id_rect, align, &app.textures);
        self.paint_player_action(player, ui, &id_rect, align);
        self.paint_winning_hand(player, ui, &id_rect, align, &app.textures);
    }

    fn paint_player_id(&self, player: &Player, ui: &mut Ui, rect: &Rect, align: &Align2) -> Rect {
        let rect = rect.shrink(5.0);

        let layout_job = text::LayoutJob {
            wrap: text::TextWrapping::wrap_at_width(75.0),
            ..text::LayoutJob::single_section(
                player.player_id_digits.clone(),
                TextFormat {
                    font_id: FontId::new(13.0, FontFamily::Monospace),
                    extra_letter_spacing: 1.0,
                    color: Self::TEXT_COLOR,
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

        if let Some(timer) = player.action_timer {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                timer.to_string(),
                FontId::new(50.0, FontFamily::Monospace),
                Self::TEXT_COLOR,
            );
        } else {
            let text_pos = rect.left_top();
            ui.painter().galley(text_pos, galley, Color32::DARK_GRAY);
        }

        if !player.is_active {
            fill_inactive(ui, &bg_rect);
        }

        bg_rect
    }

    fn paint_player_name_and_chips(&self, player: &Player, ui: &mut Ui, rect: &Rect) {
        let bg_rect = Rect::from_min_size(
            rect.left_bottom() + vec2(0.0, 10.0),
            vec2(rect.width(), 40.0),
        );

        paint_border(ui, &bg_rect);

        let painter = ui.painter().with_clip_rect(bg_rect.shrink(3.0));

        let font = FontId::new(13.0, FontFamily::Monospace);

        let galley = ui.painter().layout_no_wrap(
            player.nickname.to_string(),
            font.clone(),
            Self::TEXT_COLOR,
        );

        painter.galley(
            bg_rect.left_top() + vec2(5.0, 4.0),
            galley.clone(),
            Self::TEXT_COLOR,
        );

        let chips_pos = bg_rect.left_top() + vec2(0.0, galley.size().y);

        let galley = ui
            .painter()
            .layout_no_wrap(player.chips.to_string(), font, Self::TEXT_COLOR);

        painter.galley(chips_pos + vec2(5.0, 7.0), galley.clone(), Self::TEXT_COLOR);

        if player.has_button {
            let btn_pos = bg_rect.right_top() + vec2(-10.0, 10.0);
            painter.circle(btn_pos, 6.0, Self::TEXT_COLOR, Stroke::NONE);
        }

        if !player.is_active {
            fill_inactive(ui, &bg_rect);
        }
    }

    fn paint_player_cards(
        &self,
        player: &Player,
        ui: &mut Ui,
        rect: &Rect,
        align: &Align2,
        textures: &Textures,
    ) {
        if !player.is_active {
            return;
        }

        let (tx1, tx2) = match player.cards {
            PlayerCards::None => return,
            PlayerCards::Covered => (textures.back(), textures.back()),
            PlayerCards::Cards(c1, c2) => (textures.card(c1), textures.card(c2)),
        };

        let cards_rect = if let Align::RIGHT = align.x() {
            Rect::from_min_size(
                rect.left_top() - vec2(rect.size().x + 10.0, 0.0),
                rect.size(),
            )
        } else {
            Rect::from_min_size(rect.right_top() + vec2(10.0, 0.0), rect.size())
        };

        paint_border(ui, &cards_rect);

        let card_lx = (rect.size().x - 10.0) / 2.0;
        let card_size = vec2(card_lx, rect.size().y - 8.0);

        let card_pos = cards_rect.left_top() + vec2(4.0, 4.0);
        let c1_rect = Rect::from_min_size(card_pos, card_size);
        Image::new(&tx1).corner_radius(2.0).paint_at(ui, c1_rect);

        let c2_rect = Rect::from_min_size(card_pos + vec2(card_size.x + 2.0, 0.0), card_size);
        Image::new(&tx2).corner_radius(2.0).paint_at(ui, c2_rect);
    }

    fn paint_winning_hand(
        &self,
        player: &Player,
        ui: &mut Ui,
        rect: &Rect,
        align: &Align2,
        textures: &Textures,
    ) {
        const IMAGE_LY: f32 = 60.0;
        const LABEL_LY: f32 = 20.0;

        if let Some(payoff) = &player.payoff {
            if payoff.cards.is_empty() {
                return;
            }

            let x_pos = if let Align::RIGHT = align.x() {
                rect.left_top().x - rect.size().x - 10.0
            } else {
                rect.left_top().x
            };

            let y_pos = if let Align::TOP = align.y() {
                rect.left_top().y + 130.0
            } else {
                rect.left_top().y - (IMAGE_LY + LABEL_LY + 10.0)
            };

            let cards_rect = Rect::from_min_size(
                pos2(x_pos, y_pos),
                vec2(Self::ACTION_BUTTON_LX * 2.0 + 10.0, IMAGE_LY + LABEL_LY),
            );

            paint_border(ui, &cards_rect);

            let card_lx = (cards_rect.size().x - 11.0) / 5.0;
            let card_size = vec2(card_lx, IMAGE_LY - 8.0);
            let mut card_rect =
                Rect::from_min_size(cards_rect.left_top() + vec2(4.0, 4.0), card_size);

            for card in &payoff.cards {
                let tx = textures.card(*card);
                Image::new(&tx).corner_radius(2.0).paint_at(ui, card_rect);

                card_rect = card_rect.translate(vec2(card_lx + 1.0, 0.0));
            }

            let rank_rect = Rect::from_min_size(
                pos2(x_pos, y_pos + IMAGE_LY - 2.0),
                vec2(cards_rect.width(), LABEL_LY),
            );

            let rounding = CornerRadius {
                sw: 4,
                se: 4,
                ..CornerRadius::default()
            };

            ui.painter().rect(
                rank_rect.shrink2(vec2(2.0, 0.0)),
                rounding,
                Self::TEXT_COLOR,
                Stroke::NONE,
                StrokeKind::Inside,
            );

            ui.painter().text(
                rank_rect.center(),
                Align2::CENTER_CENTER,
                &payoff.rank,
                FontId::new(14.0, FontFamily::Monospace),
                Self::BG_COLOR,
            );
        }
    }

    fn paint_player_action(&self, player: &Player, ui: &mut Ui, rect: &Rect, align: &Align2) {
        if matches!(player.cards, PlayerCards::None) {
            return;
        }

        let rect = match align.x() {
            Align::RIGHT => Rect::from_min_size(
                rect.left_bottom() + vec2(-(rect.width() + 10.0), 10.0),
                vec2(rect.width(), 40.0),
            ),
            _ => Rect::from_min_size(
                rect.left_bottom() + vec2(rect.width() + 10.0, 10.0),
                vec2(rect.width(), 40.0),
            ),
        };

        paint_border(ui, &rect);

        if !matches!(player.action, PlayerAction::None) || player.payoff.is_some() {
            let mut action_rect = rect.shrink(1.0);
            action_rect.set_height(rect.height() / 2.0);

            let rounding = CornerRadius {
                nw: 4,
                ne: 4,
                ..CornerRadius::default()
            };

            ui.painter().rect(
                action_rect,
                rounding,
                Self::TEXT_COLOR,
                Stroke::NONE,
                StrokeKind::Inside,
            );

            let label = if player.payoff.is_some() {
                "WINNER"
            } else {
                player.action.label()
            };

            ui.painter().text(
                rect.left_top() + vec2(5.0, 3.0),
                Align2::LEFT_TOP,
                label,
                FontId::new(13.0, FontFamily::Monospace),
                Self::BG_COLOR,
            );

            if player.bet > Chips::ZERO || player.payoff.is_some() {
                let amount_rect = action_rect.translate(vec2(3.0, action_rect.height() + 2.0));

                let amount = if player.bet > Chips::ZERO {
                    player.bet.to_string()
                } else {
                    player
                        .payoff
                        .as_ref()
                        .map(|p| p.chips.to_string())
                        .unwrap_or_default()
                };

                let galley = ui.painter().layout_no_wrap(
                    amount,
                    FontId::new(13.0, FontFamily::Monospace),
                    Self::TEXT_COLOR,
                );

                ui.painter()
                    .galley(amount_rect.left_top(), galley.clone(), Self::TEXT_COLOR);
            }
        }
    }

    fn paint_action_controls(&mut self, ui: &mut Ui, rect: &Rect, app: &mut App) {
        let mut send_action = None;

        if let Some(req) = self.game_state.action_request() {
            let rect = player_rect(rect, &Align2::CENTER_BOTTOM);

            let mut btn_rect = Rect::from_min_size(
                rect.left_top() + vec2(0.0, 130.0),
                vec2(Self::ACTION_BUTTON_LX, Self::ACTION_BUTTON_LY),
            );

            for action in &req.actions {
                paint_border(ui, &btn_rect);

                let label = match action {
                    PlayerAction::Bet | PlayerAction::Raise if self.bet_params.is_some() => {
                        // Set the label for bet and raise to confirm if betting
                        // controls are active.
                        "CONFIRM"
                    }
                    _ => action.label(),
                };

                let btn = Button::new(
                    RichText::new(label)
                        .font(Self::TEXT_FONT)
                        .color(Self::TEXT_COLOR),
                )
                .fill(Self::BG_COLOR);

                let clicked = ui.put(btn_rect.shrink(2.0), btn).clicked();
                match action {
                    PlayerAction::Call | PlayerAction::Check => {
                        if ui.input(|i| i.key_pressed(Key::C)) || clicked {
                            send_action = Some((*action, Chips::ZERO));
                            self.bet_params = None;
                            break;
                        }
                    }
                    PlayerAction::Fold => {
                        if ui.input(|i| i.key_pressed(Key::F)) || clicked {
                            send_action = Some((*action, Chips::ZERO));
                            self.bet_params = None;
                            break;
                        }
                    }
                    PlayerAction::Bet | PlayerAction::Raise => {
                        if ui.input(|i| i.key_pressed(Key::Enter)) || clicked {
                            if let Some(params) = &self.bet_params {
                                send_action = Some((*action, params.raise_value.into()));
                                self.bet_params = None;
                                break;
                            }
                        }

                        if (ui.input(|i| i.key_pressed(Key::B))
                            || ui.input(|i| i.key_pressed(Key::R))
                            || clicked)
                            && self.bet_params.is_none()
                        {
                            self.bet_params = Some(BetParams {
                                min_raise: req.min_raise.into(),
                                big_blind: req.big_blind.into(),
                                raise_value: req.min_raise.into(),
                            });
                        }
                    }
                    _ => {}
                }

                btn_rect = btn_rect.translate(vec2(Self::ACTION_BUTTON_LX + 10.0, 0.0));
            }

            self.paint_betting_controls(ui, &rect);
        }

        if let Some((action, amount)) = send_action {
            let msg = Message::ActionResponse { action, amount };
            app.send_message(msg);

            self.game_state.reset_action_request();
        }
    }

    fn paint_betting_controls(&mut self, ui: &mut Ui, rect: &Rect) {
        const TEXT_FONT: FontId = FontId::new(15.0, FontFamily::Monospace);

        if let Some(params) = self.bet_params.as_mut() {
            let rect = Rect::from_min_size(
                rect.left_top() + vec2(182.0, 0.0),
                vec2(Self::ACTION_BUTTON_LX, 120.0),
            );

            paint_border(ui, &rect);

            let mut ypos = 5.0;

            ui.painter().text(
                rect.left_top() + vec2(7.0, ypos),
                Align2::LEFT_TOP,
                "Raise To",
                FontId::new(14.0, FontFamily::Monospace),
                Self::TEXT_COLOR,
            );

            let galley = ui.painter().layout_no_wrap(
                Chips::from(params.raise_value).to_string(),
                FontId::new(14.0, FontFamily::Monospace),
                Self::TEXT_COLOR,
            );

            ypos += 35.0;
            ui.painter().galley(
                rect.left_top() + vec2((rect.width() - galley.size().x) / 2.0, ypos),
                galley,
                Self::TEXT_COLOR,
            );

            let big_blind = params.big_blind;

            // Maximum bet is the local player chips.
            let max_bet = self
                .game_state
                .players()
                .first()
                .map(|p| (p.chips + p.bet).into())
                .unwrap();

            // Handle case when minimum raise is greater than this player chips, so
            // that the player can go all in.
            let min_raise = params.min_raise.min(max_bet);
            let slider = Slider::new(&mut params.raise_value, min_raise..=max_bet)
                .show_value(false)
                .step_by(big_blind as f64)
                .trailing_fill(true);

            ui.style_mut().spacing.slider_width = rect.width() - 10.0;
            ui.visuals_mut().selection.bg_fill = Self::TEXT_COLOR;

            ypos += 35.0;
            let slider_rect =
                Rect::from_min_size(rect.left_top() + vec2(5.0, ypos), vec2(rect.width(), 20.0));
            ui.put(slider_rect, slider);

            // Adjust slider value in case it goes above max_bet, this may happen if
            // the max_bet is not a multiple of the slider step_by.
            params.raise_value = params.raise_value.min(max_bet);

            ypos += 20.0;
            let btn = Button::new(RichText::new("-").font(TEXT_FONT).color(Self::TEXT_COLOR))
                .fill(Self::BG_COLOR);
            let btn_rect = Rect::from_min_size(
                rect.left_top() + vec2(0.0, ypos),
                vec2(rect.width() / 2.0 - 2.0, 20.0),
            );

            // Button click, down arrow or left arrow subtracts 1 big blind.
            if ui.put(btn_rect, btn).clicked()
                || ui.input(|i| i.key_pressed(Key::ArrowDown))
                || ui.input(|i| i.key_pressed(Key::ArrowLeft))
            {
                params.raise_value = params.raise_value.saturating_sub(big_blind).max(min_raise);
            }

            // Page down to subtract 4 big blinds
            if ui.input(|i| i.key_pressed(Key::PageDown)) {
                params.raise_value = params
                    .raise_value
                    .saturating_sub(big_blind * 4)
                    .max(min_raise);
            }

            let btn = Button::new(RichText::new("+").font(TEXT_FONT).color(Self::TEXT_COLOR))
                .fill(Self::BG_COLOR);
            let btn_rect = Rect::from_min_size(
                rect.left_top() + vec2(rect.width() / 2.0, ypos),
                vec2(rect.width() / 2.0, 20.0),
            );

            // Button click, up arrow or right arrow adds 1 big blind.
            if ui.put(btn_rect, btn).clicked()
                || ui.input(|i| i.key_pressed(Key::ArrowUp))
                || ui.input(|i| i.key_pressed(Key::ArrowRight))
            {
                params.raise_value = params.raise_value.saturating_add(big_blind).min(max_bet);
            }

            // Page up to add 4 big blinds
            if ui.input(|i| i.key_pressed(Key::PageUp)) {
                params.raise_value = params
                    .raise_value
                    .saturating_add(big_blind * 4)
                    .min(max_bet);
            }
        }
    }

    fn paint_close_button(&self, ui: &mut Ui, rect: &Rect, app: &mut App) {
        let btn = Button::new(
            RichText::new("X")
                .font(Self::TEXT_FONT)
                .color(Self::TEXT_COLOR),
        )
        .fill(Self::BG_COLOR);

        let rect = Rect::from_min_size(rect.left_top(), Self::SMALL_BUTTON_SZ);
        if ui.put(rect, btn).clicked() {
            app.send_message(Message::LeaveTable);
        }
    }

    fn paint_help_button(&mut self, ui: &mut Ui, rect: &Rect) {
        let btn = Button::new(
            RichText::new("?")
                .font(Self::TEXT_FONT)
                .color(Self::TEXT_COLOR),
        )
        .fill(Self::BG_COLOR);

        let rect = Rect::from_min_size(
            rect.right_top() - vec2(Self::SMALL_BUTTON_SZ.x, 0.0),
            Self::SMALL_BUTTON_SZ,
        );
        if ui.put(rect, btn).clicked() {
            self.show_legend ^= true;
        }
    }

    fn paint_legend(&mut self, ui: &mut Ui, rect: &Rect) {
        const LINES: &str = indoc::indoc! {r#"
            C     Call/Check
            F     Fold
            R     Raise
            B     Bet
            Up    +1BB
            Dn    -1BB
            PgUp  +4BB
            PgDn  -4BB
            Enter Confirm
            ?     Show/Hide"#};

        if ui.input(|i| i.key_pressed(Key::Questionmark)) {
            self.show_legend ^= true;
        }

        if self.show_legend {
            let rect = player_rect(rect, &Align2::CENTER_BOTTOM);
            let rect = rect.shrink(5.0);

            let layout_job = text::LayoutJob::single_section(
                LINES.to_string(),
                TextFormat {
                    font_id: FontId::new(13.0, FontFamily::Monospace),
                    color: Self::TEXT_COLOR,
                    ..Default::default()
                },
            );

            let galley = ui.painter().layout_job(layout_job);
            let min_pos = rect.left_top() - vec2(galley.size().x + 20.0, 0.0);

            // Paint peer id rect.
            let rect = Rect::from_min_size(min_pos, galley.rect.size());

            let bg_rect = rect.expand(5.0);
            paint_border(ui, &bg_rect);

            let text_pos = rect.left_top();
            ui.painter().galley(text_pos, galley, Color32::DARK_GRAY);
        }
    }

    fn paint_server_key(&self, ui: &mut Ui, rect: &Rect) {
        let layout_job = text::LayoutJob::single_section(
            format!("Server: {}", self.game_state.server_key()),
            TextFormat {
                font_id: Self::TEXT_FONT,
                color: Self::TEXT_COLOR,
                ..Default::default()
            },
        );

        let galley = ui.painter().layout_job(layout_job);

        const BORDER: f32 = 4.0;
        let text_size = galley.rect.size() + Vec2::splat(BORDER * 2.0);
        let text_pos = rect.left_bottom() + vec2(0.0, -text_size.y);
        let rect = Rect::from_min_size(text_pos, text_size);

        ui.painter().rect(
            rect,
            CornerRadius {
                ne: 5,
                ..Default::default()
            },
            Color32::from_gray(20),
            Stroke::NONE,
            StrokeKind::Inside,
        );

        ui.painter()
            .galley(text_pos + Vec2::splat(BORDER), galley, Color32::DARK_GRAY);
    }
}

fn paint_border(ui: &mut Ui, rect: &Rect) {
    let border_color = Color32::from_gray(20);
    ui.painter()
        .rect(*rect, 5.0, border_color, Stroke::NONE, StrokeKind::Inside);

    for (idx, &color) in (0..6).zip(&[100, 120, 140, 100, 80]) {
        let border_rect = rect.expand(idx as f32);
        let stroke = Stroke::new(1.0, Color32::from_gray(color as u8));
        ui.painter()
            .rect_stroke(border_rect, 5.0, stroke, StrokeKind::Inside);
    }
}

fn fill_inactive(ui: &mut Ui, rect: &Rect) {
    ui.painter().rect(
        *rect,
        2.0,
        Color32::from_rgba_unmultiplied(60, 60, 60, 140),
        Stroke::NONE,
        StrokeKind::Inside,
    );
}

fn player_rect(rect: &Rect, align: &Align2) -> Rect {
    const PLAYER_SIZE: Vec2 = vec2(120.0, 160.0);

    let rect = rect.shrink(20.0);
    let x = match align.x() {
        Align::LEFT => rect.left(),
        Align::Center => rect.center().x - PLAYER_SIZE.x / 1.5,
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

    Rect::from_min_size(pos2(x, y), PLAYER_SIZE)
}
