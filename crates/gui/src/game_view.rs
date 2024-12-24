// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Game view.
use eframe::egui::*;
use log::{error, info};

use crate::{App, ConnectView, ConnectionEvent, View};

/// Connect view.
#[derive(Default)]
pub struct GameView {
    error: String,
    connection_closed: bool,
}

impl GameView {
    fn paint_table(&self, ui: &mut Ui, rect: &Rect) {
        fn paint_shape(ui: &mut Ui, rect: &Rect, fill: Color32) {
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
        paint_shape(ui, rect, Color32::from_rgb(200, 160, 80));

        // Table pad
        let mut outer = Color32::from_rgb(90, 90, 105);
        let inner = Color32::from_rgb(15, 15, 50);
        for pad in (2..45).step_by(3) {
            paint_shape(ui, &rect.shrink(pad as f32), outer);
            outer = outer.lerp_to_gamma(inner, 0.1);
        }

        // Inner pad border
        paint_shape(ui, &rect.shrink(50.0), Color32::from_rgb(200, 160, 80));

        // Outer table
        let mut outer = Color32::from_rgb(40, 110, 20);
        let inner = Color32::from_rgb(10, 140, 10);
        for pad in (52..162).step_by(5) {
            paint_shape(ui, &rect.shrink(pad as f32), outer);
            outer = outer.lerp_to_gamma(inner, 0.1);
        }

        // Cards board
        paint_shape(ui, &rect.shrink(162.0), Color32::from_gray(160));
        paint_shape(ui, &rect.shrink(164.0), inner);
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
                    self.error = format!("Connection error {e}");
                    error!("Connection error {e}");
                }
                ConnectionEvent::Message(msg) => {
                    info!("Get message: {msg:?}");
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
            });
    }

    fn next(
        &mut self,
        _ctx: &Context,
        frame: &mut eframe::Frame,
        _app: &mut App,
    ) -> Option<Box<dyn View>> {
        if self.connection_closed {
            Some(Box::new(ConnectView::new(frame.storage())))
        } else {
            None
        }
    }
}
