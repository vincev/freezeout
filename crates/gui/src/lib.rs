// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker GUI client.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]

pub mod connect_view;

pub mod connection;

pub mod gui;
pub use gui::{AppFrame, Config};
