// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker core types shared by client and server.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]

#[cfg(feature = "connection")]
pub mod connection;
pub mod crypto;
pub mod message;
pub mod poker;
