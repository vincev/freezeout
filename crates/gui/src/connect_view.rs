// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Connection dialog view.
use eframe::egui::*;
use log::{error, info};
use serde::{Deserialize, Serialize};

use freezeout_core::{crypto::SigningKey, message::Message};

use crate::{App, ConnectionEvent, GameView, View};

const TEXT_FONT: FontId = FontId::new(15.0, FontFamily::Monospace);
const LABEL_FONT: FontId = FontId::new(16.0, FontFamily::Monospace);
const APP_KEY: &str = "freezeout";

/// Connect view.
pub struct ConnectView {
    passphrase: String,
    player_id: String,
    nickname: String,
    error: String,
    connection_open: bool,
}

/// Data persisted across sessions.
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageData {
    /// The last saved passphrase.
    pub passphrase: String,
    /// The last saved nickname.
    pub nickname: String,
}

impl Default for ConnectView {
    fn default() -> Self {
        let sk = SigningKey::default();
        Self {
            passphrase: sk.phrase(),
            player_id: sk.verifying_key().peer_id().to_string(),
            nickname: String::default(),
            error: String::default(),
            connection_open: false,
        }
    }
}

impl ConnectView {
    /// Creates a new connect view.
    pub fn new(storage: Option<&dyn eframe::Storage>) -> Self {
        storage
            .and_then(|s| eframe::get_value::<StorageData>(s, APP_KEY))
            .map(|d| {
                let sk = SigningKey::from_phrase(&d.passphrase).unwrap_or_default();
                ConnectView {
                    passphrase: sk.phrase(),
                    player_id: sk.verifying_key().peer_id().to_string(),
                    nickname: d.nickname,
                    error: String::new(),
                    connection_open: false,
                }
            })
            .unwrap_or_default()
    }
}

impl View for ConnectView {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame, app: &mut App) {
        while let Some(event) = app.poll_network() {
            match event {
                ConnectionEvent::Open => {
                    app.send_message(Message::JoinTable(self.nickname.to_string()));
                    self.connection_open = true;
                }
                ConnectionEvent::Close => {
                    self.error = "Connection closed".to_string();
                }
                ConnectionEvent::Error(e) => {
                    self.error = format!("Connection error {e}");
                }
                ConnectionEvent::Message(msg) => {
                    info!("Get message: {msg:?}");
                }
            }
        }

        Window::new("Login")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 180.0))
            .show(ctx, |ui| {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Nickname").font(LABEL_FONT));
                        TextEdit::singleline(&mut self.nickname)
                            .hint_text("Nickname")
                            .char_limit(10)
                            .desired_width(350.0)
                            .font(TEXT_FONT)
                            .show(ui);
                    });
                });

                ui.add_space(10.0);

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Passphrase").font(LABEL_FONT));
                        ui.add_space(180.0);
                        if ui
                            .button(RichText::new("Generate").font(TEXT_FONT))
                            .clicked()
                        {
                            self.error.clear();

                            let sk = SigningKey::default();
                            self.passphrase = sk.phrase();
                            self.player_id = sk.verifying_key().peer_id().to_string();
                        }
                    });

                    // Copy passphrase from clipboard by replacing current text
                    ui.input(|i| {
                        for event in &i.events {
                            if let Event::Paste(text) = event {
                                if let Ok(sk) = SigningKey::from_phrase(text) {
                                    self.error.clear();
                                    self.passphrase = sk.phrase();
                                    self.player_id = sk.verifying_key().peer_id().to_string();
                                } else {
                                    self.error = "Invalid clipboard passphrase".to_string();
                                }
                            }
                        }
                    });

                    let mut passphrase = self.passphrase.clone();
                    TextEdit::multiline(&mut passphrase)
                        .hint_text("Passphrase")
                        .char_limit(215)
                        .desired_rows(5)
                        .desired_width(350.0)
                        .font(TEXT_FONT)
                        .show(ui);

                    ui.label(RichText::new("Player ID").font(LABEL_FONT));
                    let mut player_id = self.player_id.clone();
                    TextEdit::singleline(&mut player_id)
                        .hint_text("Player ID")
                        .desired_width(350.0)
                        .font(TEXT_FONT)
                        .show(ui);
                });

                if !self.error.is_empty() {
                    ui.label(
                        RichText::new(&self.error)
                            .font(TEXT_FONT)
                            .color(Color32::RED),
                    );
                }

                ui.add_space(10.0);

                let btn = ui.button(RichText::new("Connect").font(TEXT_FONT));
                if btn.clicked() {
                    self.error.clear();

                    if self.nickname.trim().is_empty() {
                        self.error = "Invalid nickname".to_string();
                        return;
                    }

                    let sk = if let Ok(sk) = SigningKey::from_phrase(&self.passphrase) {
                        let data = StorageData {
                            passphrase: self.passphrase.clone(),
                            nickname: self.nickname.clone(),
                        };

                        if let Some(storage) = frame.storage_mut() {
                            eframe::set_value::<StorageData>(storage, APP_KEY, &data);
                            storage.flush();
                        }

                        sk
                    } else {
                        self.error = "Invalid passphrase".to_string();
                        return;
                    };

                    if let Err(e) = app.connect(sk, self.nickname.trim(), ctx) {
                        self.error = "Connect error".to_string();
                        error!("Connect error: {e}");
                    }
                }
            });
    }

    fn next(
        &mut self,
        ctx: &Context,
        _frame: &mut eframe::Frame,
        _app: &mut App,
    ) -> Option<Box<dyn View>> {
        if self.connection_open {
            Some(Box::new(GameView::new(ctx)))
        } else {
            None
        }
    }
}
