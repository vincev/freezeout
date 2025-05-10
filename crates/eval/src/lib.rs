// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker hand evaluator.
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
pub mod eval;
pub use eval::{HandRank, HandValue};

// Reexport cards types.
pub use freezeout_cards::{Card, Deck, Rank, Suit};
