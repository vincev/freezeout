// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker egui app implementation.
use anyhow::Result;
use eframe::egui::*;
use serde::{Deserialize, Serialize};

use freezeout_cards::egui::Textures;
use freezeout_core::{
    crypto::{PeerId, SigningKey},
    message::{Message, SignedMessage},
};

use crate::{ConnectView, Connection, ConnectionEvent};

/// App configuration parameters.
#[derive(Debug)]
pub struct Config {
    /// The server address in 'host:port' format.
    pub server_url: String,
}

/// Data persisted across sessions.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppData {
    /// The last saved passphrase.
    pub passphrase: String,
    /// The last saved nickname.
    pub nickname: String,
}

/// The application state shared by all views.
pub struct App {
    /// The application configuration.
    pub config: Config,
    /// The app textures.
    pub textures: Textures,
    /// The application message signing key.
    sk: SigningKey,
    /// This client player id.
    player_id: PeerId,
    /// This client nickname
    nickname: String,
    /// This client connection.
    connection: Option<Connection>,
}

impl App {
    const STORAGE_KEY: &str = "appdata";

    fn new(config: Config, textures: Textures) -> Self {
        let sk = SigningKey::default();
        Self {
            config,
            textures,
            player_id: sk.verifying_key().peer_id(),
            sk,
            nickname: String::default(),
            connection: None,
        }
    }

    /// Connects to a server.
    pub fn connect(&mut self, sk: SigningKey, nickname: &str, ctx: &Context) -> Result<()> {
        let con = Connection::connect(&self.config.server_url, ctx.clone())?;

        if let Some(mut c) = self.connection.take() {
            c.close();
        }

        self.connection = Some(con);
        self.player_id = sk.verifying_key().peer_id();
        self.sk = sk;
        self.nickname = nickname.to_string();

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

    /// This client player id.
    pub fn player_id(&self) -> &PeerId {
        &self.player_id
    }

    /// This client nickname.
    pub fn nickname(&self) -> &str {
        &self.nickname
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

    /// Get a value from the app storage.
    pub fn get_storage(&self, storage: Option<&dyn eframe::Storage>) -> Option<AppData> {
        storage.and_then(|s| eframe::get_value::<AppData>(s, Self::STORAGE_KEY))
    }

    /// Set a value in the app storage.
    pub fn set_storage(
        &self,
        storage: Option<&mut (dyn eframe::Storage + 'static)>,
        data: &AppData,
    ) {
        if let Some(s) = storage {
            eframe::set_value::<AppData>(s, Self::STORAGE_KEY, data);
            s.flush();
        }
    }
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

impl AppFrame {
    /// Creates a new App instance.
    pub fn new(config: Config, cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(Theme::Dark);

        log::info!("Creating new app with config: {config:?}");
        let app = App::new(config, Textures::new(&cc.egui_ctx));
        let panel = Box::new(ConnectView::new(cc.storage, &app));

        AppFrame { app, panel }
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
