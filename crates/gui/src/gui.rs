// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker egui app implementation.
use eframe::egui::*;

/// App configuration parameters.
#[derive(Debug)]
pub struct Config {
    /// The server address in 'host:port' format.
    pub server_address: String,
}

/// The client App implementation.
pub struct App {
    config: Config,
}

impl App {
    /// Creates a new App instance.
    pub fn new(config: Config, cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(Theme::Dark);

        log::info!("Creating new app with config: {config:?}");

        App { config }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        Window::new("App window")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 180.0))
            .show(ctx, |ui| {
                ui.label(format!("Connect to server: {}", self.config.server_address))
            });
    }
}
