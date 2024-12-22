// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Connection dialog view.
use eframe::egui::*;

use freezeout_core::{crypto::SigningKey, message::Message};

use crate::gui::{App, View};

/// Connect view.
#[derive(Default)]
pub struct ConnectView {
    error: Option<String>,
}

impl View for ConnectView {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame, app: &mut App) {
        while let Some(event) = app.poll_network() {
            log::info!("Got event {event:?}");
        }

        Window::new("App window")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 180.0))
            .show(ctx, |ui| {
                if app.is_connected() {
                    if ui.button("Send Join Table").clicked() {
                        app.send_message(Message::JoinTable("Bob".to_string()));
                    }
                } else if ui
                    .button(format!("Connect to: {}", app.config.server_address))
                    .clicked()
                {
                    self.error = None;
                    let url = format!("ws://{}", app.config.server_address);
                    let sk = SigningKey::default();

                    if let Err(e) = app.connect(&url, sk, ctx) {
                        self.error = Some(e.to_string());
                    }
                }
            });
    }

    fn next(
        &mut self,
        _ctx: &Context,
        _frame: &mut eframe::Frame,
        _app: &mut App,
    ) -> Option<Box<dyn View>> {
        None
    }
}
