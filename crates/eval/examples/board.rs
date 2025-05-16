// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
//
// ```bash
// $ cargo r --release --example board
// ```
use eframe::egui::{self, pos2, vec2};
use std::{sync::mpsc, thread};

use freezeout_cards::{egui::Textures, *};
use freezeout_eval::eval::*;

struct Sim {
    pair: Vec<Card>,
    board: Vec<Card>,
    num_players: usize,
}

impl Sim {
    fn run(&self) -> f64 {
        const SAMPLES: usize = 10_000;
        const BOARD_SIZE: usize = 5;
        const BOARD_IDX: usize = 2;

        assert!(self.num_players > 0);
        assert!(self.pair.len() == 2);
        assert!(self.board.len() <= 5);

        // Remove cards from the deck so that we don't sample them.
        let mut deck = Deck::default();

        // Removes pair cards from the deck.
        for c in &self.pair {
            deck.remove(*c);
        }

        // Removes board cards from the deck.
        for c in &self.board {
            deck.remove(*c);
        }

        let sample_size = 2 * self.num_players + BOARD_SIZE;
        let mut wins = 0;
        let mut games = 0;

        deck.sample(SAMPLES, sample_size, |sample| {
            // The sample contains the two cards for each player and the board cards,
            // copy the board cards to the end of the evaluation array.
            let mut hand = [Card::default(); BOARD_SIZE + 2];
            let board_start = self.num_players * BOARD_IDX;
            hand[2..].copy_from_slice(&sample[board_start..]);

            for (i, c) in self.board.iter().enumerate() {
                hand[i + BOARD_IDX] = *c;
            }

            // Evaluate hero hand.
            hand[0] = self.pair[0];
            hand[1] = self.pair[1];
            let hvalue = HandValue::eval(&hand);

            // Compare against other players hand.
            let mut has_lost = false;
            for player in 0..self.num_players {
                hand[0] = sample[player * 2];
                hand[1] = sample[player * 2 + 1];
                let ovalue = HandValue::eval(&hand);
                if ovalue > hvalue {
                    has_lost = true;
                    break;
                }
            }

            if !has_lost {
                wins += 1;
            }

            games += 1;
        });

        wins as f64 / games as f64
    }
}

struct App {
    textures: Textures,
    player_cards: Vec<Card>,
    board_cards: Vec<Card>,
    deck: Vec<(Card, bool)>,
    num_players: usize,
    win_prob: Option<f64>,
    task_tx: Option<mpsc::Sender<Sim>>,
    task_rx: mpsc::Receiver<f64>,
    task: Option<thread::JoinHandle<()>>,
}

impl App {
    const FRAME_SIZE: egui::Vec2 = vec2(600.0, 680.0);
    const CARD_SIZE: egui::Vec2 = vec2(30.0, 56.0);
    const DECK_ROW_LX: f32 = Self::CARD_SIZE.x * 13.0 + 120.0;
    const BOARD_ROW_LX: f32 = Self::CARD_SIZE.x * 5.0 + 40.0;
    const BTN_LY: f32 = 50.0;

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (task_tx, app_rx) = mpsc::channel::<Sim>();
        let (app_tx, task_rx) = mpsc::channel();

        let ctx = cc.egui_ctx.clone();
        let task = thread::spawn(move || {
            while let Ok(sim) = app_rx.recv() {
                let result = sim.run();
                if app_tx.send(result).is_err() {
                    break;
                }

                ctx.request_repaint();
            }
        });

        let mut app = Self {
            textures: Textures::new(&cc.egui_ctx),
            player_cards: Vec::default(),
            board_cards: Vec::default(),
            deck: Vec::default(),
            num_players: 1,
            win_prob: Some(0.42),
            task_tx: Some(task_tx),
            task_rx,
            task: Some(task),
        };

        app.reset_cards();
        app
    }

    /// Paint the deck cards one row per suit.
    fn paint_deck(&mut self, ui: &mut egui::Ui, pos: egui::Pos2) {
        let mut row_pos = pos;
        self.paint_deck_row(ui, row_pos, Suit::Hearts);

        row_pos.y += App::CARD_SIZE.y + 10.0;
        self.paint_deck_row(ui, row_pos, Suit::Diamonds);

        row_pos.y += App::CARD_SIZE.y + 10.0;
        self.paint_deck_row(ui, row_pos, Suit::Clubs);

        row_pos.y += App::CARD_SIZE.y + 10.0;
        self.paint_deck_row(ui, row_pos, Suit::Spades);

        let sz = vec2(App::DECK_ROW_LX, row_pos.y + App::CARD_SIZE.y - pos.y);
        let r = egui::Rect::from_min_size(pos, sz).expand2(vec2(15.0, 20.0));
        paint_frame(ui, r, "Select cards for the player and the board");
    }

    fn paint_deck_row(&mut self, ui: &mut egui::Ui, pos: egui::Pos2, suit: Suit) {
        let mut pos = pos;
        let mut deck_changed = false;

        for (c, v) in self.deck.iter_mut().filter(|(c, _)| c.suit() == suit) {
            if *v {
                let tx = self.textures.card(*c);
                let img = egui::Image::new(&tx)
                    .max_size(App::CARD_SIZE)
                    .corner_radius(2.0);

                let card_rect = egui::Rect::from_min_size(pos, App::CARD_SIZE).expand(2.0);
                // Move card to the player or the board.
                if ui.put(card_rect, egui::Button::image(img)).clicked() {
                    if self.player_cards.len() < 2 {
                        self.player_cards.push(*c);
                        deck_changed = true;
                        *v = false;
                    } else if self.board_cards.len() < 5 {
                        self.board_cards.push(*c);
                        deck_changed = true;
                        *v = false;
                    }
                }
            }

            pos.x += App::CARD_SIZE.x + 10.0;
        }

        if deck_changed {
            self.send_sim();
        }
    }

    fn paint_player(&mut self, ui: &mut egui::Ui, pos: egui::Pos2) {
        self.paint_cards_row(ui, pos, &self.player_cards);

        let sz = vec2(App::BOARD_ROW_LX, App::CARD_SIZE.y);
        let r = egui::Rect::from_min_size(pos, sz).expand2(vec2(15.0, 20.0));
        paint_frame(ui, r, "Player cards");
    }

    fn paint_board(&mut self, ui: &mut egui::Ui, pos: egui::Pos2) {
        self.paint_cards_row(ui, pos, &self.board_cards);

        let sz = vec2(App::BOARD_ROW_LX, App::CARD_SIZE.y);
        let r = egui::Rect::from_min_size(pos, sz).expand2(vec2(15.0, 20.0));
        paint_frame(ui, r, "Board cards");
    }

    fn paint_cards_row(&self, ui: &mut egui::Ui, pos: egui::Pos2, cards: &[Card]) {
        let mut row_pos = pos;
        for c in cards {
            let tx = self.textures.card(*c);
            let img = egui::Image::new(&tx)
                .max_size(App::CARD_SIZE)
                .corner_radius(2.0);

            let card_rect = egui::Rect::from_min_size(row_pos, App::CARD_SIZE).expand(2.0);
            ui.put(card_rect, img);
            row_pos.x += App::CARD_SIZE.x + 10.0;
        }
    }

    fn paint_win_prob(&self, ui: &mut egui::Ui, pos: egui::Pos2) {
        let sz = vec2(255.0, App::CARD_SIZE.y * 3.0 + 3.0);
        let r = egui::Rect::from_min_size(pos, sz).expand(20.0);
        paint_frame(ui, r, "Winning %");

        if let Some(pct) = self.win_prob {
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}", (pct * 100.0).round()),
                egui::FontId::new(100.0, egui::FontFamily::Monospace),
                egui::Color32::from_gray(80),
            );
        }
    }

    fn paint_controls(&mut self, ui: &mut egui::Ui, pos: egui::Pos2) {
        const BTN_LX: f32 = 30.0;
        let mut btn_pos = pos;

        for n in 1..=6 {
            let btn_size = vec2(BTN_LX, Self::BTN_LY);
            let btn_rect = egui::Rect::from_min_size(btn_pos, btn_size).expand(2.0);
            let label = egui::SelectableLabel::new(n == self.num_players, n.to_string());
            if ui.put(btn_rect, label).clicked() {
                self.num_players = n;
                self.send_sim();
            }

            btn_pos.x += BTN_LX + 10.0;
        }

        let sz = vec2(BTN_LX * 6.0 + 50.0, Self::BTN_LY);
        let r = egui::Rect::from_min_size(pos, sz).expand2(vec2(15.0, 20.0));
        paint_frame(ui, r, "Number of players");

        // Add back and clear buttons.
        let r = r.translate(vec2(r.width() + 20.0, 0.0));
        let btn_rect = egui::Rect::from_min_size(
            r.left_top() + vec2(15.0, 20.0),
            vec2(r.width() / 2.0 - 20.0, Self::BTN_LY),
        );

        let label = egui::RichText::new("<--").size(16.0).strong();
        if ui.put(btn_rect, egui::Button::new(label)).clicked() {
            self.delete_one();
        }

        let btn_rect = btn_rect.translate(vec2(btn_rect.width() + 10.0, 0.0));
        let label = egui::RichText::new("Reset").size(16.0).strong();
        if ui.put(btn_rect, egui::Button::new(label)).clicked() {
            self.reset_cards();
        }

        paint_frame(ui, r, "");
    }

    fn reset_cards(&mut self) {
        self.deck = Deck::default().into_iter().map(|c| (c, true)).collect();
        self.player_cards.clear();
        self.board_cards.clear();
        self.win_prob = None;
    }

    fn delete_one(&mut self) {
        if !self.board_cards.is_empty() {
            let c = self.board_cards.pop();
            self.put_back(c.unwrap());
            self.send_sim();
        } else if !self.player_cards.is_empty() {
            let c = self.player_cards.pop();
            self.put_back(c.unwrap());
            self.send_sim();
        }

        if self.player_cards.len() != 2 {
            self.win_prob = None;
        }
    }

    fn put_back(&mut self, c: Card) {
        for (dc, v) in &mut self.deck {
            if c == *dc {
                *v = true;
                break;
            }
        }
    }

    fn send_sim(&self) {
        // Need at least player cards.
        if self.player_cards.len() == 2 {
            let sim = Sim {
                pair: self.player_cards.clone(),
                board: self.board_cards.clone(),
                num_players: self.num_players,
            };

            if let Some(tx) = self.task_tx.as_ref() {
                let _ = tx.send(sim);
            }
        }
    }
}

fn paint_frame(ui: &mut egui::Ui, rect: egui::Rect, label: &str) {
    let color = egui::Color32::from_gray(180);
    ui.painter().rect_stroke(
        rect,
        5.0,
        egui::Stroke::new(2.0, color),
        egui::StrokeKind::Outside,
    );

    if !label.is_empty() {
        let color = egui::Color32::from_gray(100);
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            egui::FontId::new(16.0, egui::FontFamily::Monospace),
            color,
        );

        let pos = rect.left_top() + vec2(30.0, -galley.size().y / 2.0);
        let bg_rect = egui::Rect::from_min_size(pos, galley.size()).expand(5.0);
        let bg_color = ui.style().visuals.window_fill;
        ui.painter().rect_filled(bg_rect, 5.0, bg_color);
        ui.painter().galley(pos, galley, color);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(win_prob) = self.task_rx.try_recv() {
            self.win_prob = Some(win_prob);
        }

        egui::CentralPanel::default().show(&ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Self::FRAME_SIZE, egui::Sense::hover());

            let start_x = (rect.width() - App::DECK_ROW_LX) / 2.0;
            let mut pos = pos2(start_x, 40.0);
            self.paint_player(ui, pos);
            self.paint_win_prob(ui, pos + vec2(App::BOARD_ROW_LX + 60.0, 0.0));

            pos.y += App::CARD_SIZE.y + 60.0;
            self.paint_board(ui, pos);

            pos.y += App::CARD_SIZE.y + 60.0;
            self.paint_deck(ui, pos);

            pos.y += App::CARD_SIZE.y * 4.0 + 90.0;
            self.paint_controls(ui, pos);
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Drop channel to signal task to exit.
        self.task_tx = None;
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(App::FRAME_SIZE)
            .with_min_inner_size(App::FRAME_SIZE)
            .with_max_inner_size(App::FRAME_SIZE)
            .with_title("Board demo"),
        ..Default::default()
    };

    eframe::run_native(
        "board",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
