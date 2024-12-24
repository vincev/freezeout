// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker GUI client.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]

pub mod connect_view;
pub use connect_view::ConnectView;

pub mod connection;
pub use connection::{Connection, ConnectionEvent};

pub mod game_view;
pub use game_view::GameView;

pub mod gui;
pub use gui::{App, AppFrame, Config, View};
