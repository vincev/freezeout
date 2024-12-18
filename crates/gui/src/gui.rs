// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker egui app implementation.
use eframe::egui::*;

use freezeout_core::{
    crypto::SigningKey,
    message::{Message, SignedMessage},
};

use crate::connection::Connection;

/// App configuration parameters.
#[derive(Debug)]
pub struct Config {
    /// The server address in 'host:port' format.
    pub server_address: String,
}

/// The client App implementation.
pub struct App {
    config: Config,
    signing_key: SigningKey,
    connection: Option<Connection>,
    error: Option<String>,
}

impl App {
    /// Creates a new App instance.
    pub fn new(config: Config, cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(Theme::Dark);

        log::info!("Creating new app with config: {config:?}");

        App {
            config,
            signing_key: SigningKey::default(),
            connection: None,
            error: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if let Some(c) = self.connection.as_mut() {
            while let Some(event) = c.poll() {
                log::info!("Got event {event:?}");
            }
        }

        Window::new("App window")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 180.0))
            .show(ctx, |ui| {
                if let Some(c) = self.connection.as_mut() {
                    if ui.button("Send Join Table").clicked() {
                        let msg = SignedMessage::new(
                            &self.signing_key,
                            Message::JoinTable("Bob".to_string()),
                        );
                        c.send(&msg);
                    }
                } else if ui
                    .button(format!("Connect to: {}", self.config.server_address))
                    .clicked()
                {
                    self.error = None;
                    let url = format!("ws://{}", self.config.server_address);

                    match Connection::connect(&url, ctx.clone()) {
                        Ok(c) => self.connection = Some(c),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(c) = self.connection.as_mut() {
            c.close();
        }
    }
}
