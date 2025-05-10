// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker cards types.
//!
//! This crate define types to create cards:
//!
//! ```
//! # use freezeout_cards::{Card, Rank, Suit};
//! let ah = Card::new(Rank::Ace, Suit::Hearts);
//! let kd = Card::new(Rank::Ace, Suit::Diamonds);
//! ```
//!
//! and a [Deck] type for shuffling, sampling, and iterating cards in the deck.
//!
//! For example to iterate through all 7 cards hands:
//!
//! ```no_run
//! # use freezeout_cards::{Card, Deck, Rank, Suit};
//! // Iterate through all 7 cards hands (133M hands).
//! let mut counter = 0;
//! Deck::default().for_each(7, |hand| {
//!     counter += 1;
//! });
//! assert_eq!(counter, 133_784_560);
//! ```
//!
//! to sample 10 random 5-cards hands:
//!
//! ```
//! # use freezeout_cards::{Card, Deck, Rank, Suit};
//! // Iterate through all 7 cards hands (133M hands).
//! let mut counter = 0;
//! Deck::default().sample(10, 5, |hand| {
//!     assert_eq!(hand.len(), 5);
//!     counter += 1;
//! });
//! assert_eq!(counter, 10);
//! ```
//!
//! The **`parallel`** feature enables parallel sampling and iteration with
//! a given number of tasks, the following example uses 4 tasks to iterate
//! all 7 cards hands, the closure `task_id` can be used to store per task data
//! to reduce contention:
//!
//! ```
//! # #[cfg(feature = "parallel")]
//! # fn par_for_each() {
//! # use std::sync::atomic;
//! # use freezeout_cards::{Card, Deck, Rank, Suit};
//! // Iterate through all 7 cards hands (133M hands).
//! let counter = atomic::AtomicU64::new(0);
//! Deck::default().par_for_each(4, 7, |task_id, hand| {
//!     assert_eq!(hand.len(), 7);
//!     counter.fetch_add(1, atomic::Ordering::Relaxed);
//! });
//! assert_eq!(counter.load(atomic::Ordering::Relaxed), 133_784_560);
//! # }
//! ```
//!
//! for parallel sampling the following uses 4 tasks and sample 10 7-cards hand for
//! each task:
//!
//! ```
//! # #[cfg(feature = "parallel")]
//! # fn par_sample() {
//! # use std::sync::atomic;
//! # use freezeout_cards::{Card, Deck, Rank, Suit};
//! // Iterate through all 7 cards hands (133M hands).
//! let counter = atomic::AtomicU64::new(0);
//! Deck::default().par_sample(4, 10, 7, |task_id, hand| {
//!     assert_eq!(hand.len(), 7);
//!     counter.fetch_add(1, atomic::Ordering::Relaxed);
//! });
//! assert_eq!(counter.load(atomic::Ordering::Relaxed), 40);
//! # }
//! ```
//!
//! To **`egui`** feature exports the [Textures](egui::Textures) type to access
//! the cards images, see the examples code.
#[warn(clippy::all, rust_2018_idioms, missing_docs)]
mod deck;
pub use deck::{Card, Deck, Rank, Suit};

#[cfg(feature = "egui")]
pub mod egui;
