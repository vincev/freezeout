// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Bot.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]

mod client;
pub use client::{Config, Strategy, run};

pub use freezeout_core as core;
