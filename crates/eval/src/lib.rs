// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker hand evaluator.
//!
//! Poker hand evaluator for 5, 6 and 7 cards hands. This evaluator is a port of
//! the [Cactus Kev's][kevlink] poker evaluator with an additional lookup table
//! for faster 7 cards evaluation (see examples for measuring single and parallel
//! performance on you hardware).
//!
//! To use the evaluator create a hand and use [HandValue] to evaluate the hand
//! and get its rank:
//!
//! ```
//! # use freezeout_eval::*;
//! // 2C, 3C, .., JC
//! let cards = Deck::default().into_iter().take(10).collect::<Vec<_>>();
//! let v1 = HandValue::eval(&cards[0..5]);
//! let v2 = HandValue::eval(&cards[5..]);
//! assert!(v2 > v1);
//!
//! ```
//!
//! [kevlink]: http://suffe.cool/poker/evaluator.html
#![warn(clippy::all, rust_2018_idioms, missing_docs)]
pub mod eval;
pub use eval::{HandRank, HandValue};

// Reexport cards types.
pub use freezeout_cards::{Card, Deck, Rank, Suit};
