// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker server.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]

pub mod connection;
pub mod db;
pub mod server;
pub use server::{Config, run};
pub mod table;
