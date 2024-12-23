// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker egui app implementation.
use anyhow::Result;
use eframe::egui::*;

use freezeout_core::{
    crypto::SigningKey,
    message::{Message, SignedMessage},
};

use crate::{
    connect_view::ConnectView,
    connection::{Connection, ConnectionEvent},
};

/// App configuration parameters.
#[derive(Debug)]
pub struct Config {
    /// The server address in 'host:port' format.
    pub server_address: String,
}

/// The application state shared by all views.
pub struct App {
    /// The application configuration.
    pub config: Config,
    /// The application message signing key.
    pub sk: SigningKey,
    connection: Option<Connection>,
}

/// Traits for UI views.
pub trait View {
    /// Process a view update.
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame, app: &mut App);

    /// Returns the next view if any.
    fn next(
        &mut self,
        ctx: &Context,
        frame: &mut eframe::Frame,
        app: &mut App,
    ) -> Option<Box<dyn View>>;
}

/// The UI main frame.
pub struct AppFrame {
    app: App,
    panel: Box<dyn View>,
}

impl App {
    /// Connects to a server.
    pub fn connect(&mut self, sk: SigningKey, ctx: &Context) -> Result<()> {
        let url = format!("ws://{}", self.config.server_address);
        let con = Connection::connect(&url, ctx.clone())?;

        if let Some(mut c) = self.connection.take() {
            c.close();
        }

        self.connection = Some(con);
        self.sk = sk;

        Ok(())
    }

    /// Checks if there is an active connection.
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Polls the active connection.
    pub fn poll_network(&mut self) -> Option<ConnectionEvent> {
        if let Some(c) = self.connection.as_mut() {
            c.poll()
        } else {
            None
        }
    }

    /// Sends a message to the server.
    pub fn send_message(&mut self, msg: Message) {
        if let Some(c) = self.connection.as_mut() {
            let msg = SignedMessage::new(&self.sk, msg);
            c.send(&msg);
        }
    }

    /// Close the current connection.
    pub fn close_connection(&mut self) {
        if let Some(c) = self.connection.as_mut() {
            c.close();
        }
    }
}

impl AppFrame {
    /// Creates a new App instance.
    pub fn new(config: Config, cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(Theme::Dark);

        log::info!("Creating new app with config: {config:?}");

        let app = App {
            config,
            sk: SigningKey::default(),
            connection: None,
        };

        AppFrame {
            app,
            panel: Box::new(ConnectView::new(cc.storage)),
        }
    }
}

impl eframe::App for AppFrame {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.panel.update(ctx, frame, &mut self.app);

        if let Some(panel) = self.panel.next(ctx, frame, &mut self.app) {
            self.panel = panel;
            self.panel.update(ctx, frame, &mut self.app);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app.close_connection();
    }
}
