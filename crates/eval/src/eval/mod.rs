// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Poker hand evaluator.
//!
//! This evaluator is a port of the [Cactus Kev's][kevlink] poker evaluator to
//! evaluate 5, 6, and 7 cards poker hands with an additional lookup table for
//! faster 7 cards evaluation.
//!
//! It provides a [HandValue::eval] method that computes a hand rank without
//! extracting the best hand out of a 7 cards hand, useful for computing odds
//! and other stats, and a slightly slower [HandValue::eval_with_best_hand] that
//! computes the hand rank and returns the five best cards, useful for UIs to
//! shows a winning hand.
//!
//! [kevlink]: http://suffe.cool/poker/evaluator.html
//! [kevcode]: http://suffe.cool/poker/code/

pub mod eval;
pub use eval::{HandRank, HandValue};

mod eval7;
