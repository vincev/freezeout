// Copyright (C) 2025  Vince Vasta.
// SPDX-License-Identifier: Apache-2.0

//! Noise protocol encrypted WebSocket connection.
use anyhow::{Result, bail};
use eframe::egui;
use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use snow::{HandshakeState, TransportState, params::NoiseParams};
use std::sync::LazyLock;

use freezeout_core::message::SignedMessage;

static NOISE_PARAMS: LazyLock<NoiseParams> =
    LazyLock::new(|| "Noise_NN_25519_ChaChaPoly_BLAKE2s".parse().unwrap());

/// Connection to game server.
pub struct Connection {
    ws_sender: WsSender,
    ws_receiver: WsReceiver,
    noise_handshake: Option<HandshakeState>,
    noise_transport: Option<TransportState>,
    noise_buf: Vec<u8>,
}

/// Connection event.
#[derive(Debug)]
pub enum ConnectionEvent {
    /// Connection opened.
    Open,
    /// Connection closed.
    Close,
    /// Connection error.
    Error(String),
    /// Connection message.
    Message(SignedMessage),
}

impl Connection {
    /// Connect to server.
    pub fn connect(url: &str, ctx: egui::Context) -> Result<Self> {
        // Wake up UI thread on new message
        let wakeup = move || ctx.request_repaint();
        match ewebsock::connect_with_wakeup(url, Default::default(), wakeup) {
            Ok((ws_sender, ws_receiver)) => Ok(Connection {
                ws_sender,
                ws_receiver,
                noise_handshake: None,
                noise_transport: None,
                noise_buf: vec![0u8; 8192],
            }),
            Err(e) => bail!("Connection error {e}"),
        }
    }

    /// Closes this connection.
    pub fn close(&mut self) {
        self.ws_sender.close();
    }

    /// Send a message.
    pub fn send(&mut self, msg: &SignedMessage) {
        if let Some(noise) = self.noise_transport.as_mut() {
            let len = noise
                .write_message(&msg.serialize(), &mut self.noise_buf)
                .expect("Cannot write noise message");
            self.ws_sender
                .send(WsMessage::Binary(self.noise_buf[..len].to_vec()));
        }
    }

    /// Polls the connection
    pub fn poll(&mut self) -> Option<ConnectionEvent> {
        if let Some(event) = self.ws_receiver.try_recv() {
            match event {
                WsEvent::Opened => {
                    let mut noise = snow::Builder::new(NOISE_PARAMS.clone())
                        .build_initiator()
                        .expect("Cannot initiate noise protocol");

                    // Initiate noise handshake.
                    // -> e
                    let len = noise
                        .write_message(&[], &mut self.noise_buf)
                        .expect("Cannot initiate noise handshake");

                    self.ws_sender
                        .send(WsMessage::Binary(self.noise_buf[..len].to_vec()));

                    self.noise_handshake = Some(noise);
                    None
                }
                WsEvent::Message(msg) => {
                    if let WsMessage::Binary(bytes) = msg {
                        if let Some(mut noise) = self.noise_handshake.take() {
                            // Complete noise handshake.
                            // <- e, ee
                            if noise.read_message(&bytes, &mut self.noise_buf).is_err() {
                                return Some(ConnectionEvent::Error(
                                    "Cannot complete noise handshake".to_string(),
                                ));
                            }

                            let Ok(transport) = noise.into_transport_mode() else {
                                return Some(ConnectionEvent::Error(
                                    "Cannot create noise transport".to_string(),
                                ));
                            };

                            self.noise_transport = Some(transport);
                            Some(ConnectionEvent::Open)
                        } else if let Some(noise) = self.noise_transport.as_mut() {
                            let res = noise
                                .read_message(&bytes, &mut self.noise_buf)
                                .map_err(anyhow::Error::from)
                                .and_then(|len| {
                                    SignedMessage::deserialize_and_verify(&self.noise_buf[..len])
                                });

                            match res {
                                Ok(msg) => Some(ConnectionEvent::Message(msg)),
                                Err(e) => Some(ConnectionEvent::Error(e.to_string())),
                            }
                        } else {
                            self.ws_sender.close();
                            Some(ConnectionEvent::Close)
                        }
                    } else {
                        None
                    }
                }
                WsEvent::Error(e) => Some(ConnectionEvent::Error(e)),
                WsEvent::Closed => Some(ConnectionEvent::Close),
            }
        } else {
            None
        }
    }
}
