// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Types used in a Poker game.
use serde::{Deserialize, Serialize};
use std::{fmt, sync::atomic};

mod cards;
pub use cards::{Card, Deck, Rank, Suit};

/// A unique table identifier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TableId(u32);

impl TableId {
    /// A table id for an unassigned table.
    pub const NO_TABLE: TableId = TableId(0);

    /// Create a new unique table id.
    pub fn new_id() -> TableId {
        static LAST_ID: atomic::AtomicU32 = atomic::AtomicU32::new(1);
        TableId(LAST_ID.fetch_add(1, atomic::Ordering::Relaxed))
    }
}

impl fmt::Display for TableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Chips amount.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Chips(pub u32);

impl fmt::Display for Chips {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let amount = self.0;
        if amount >= 10_000_000 {
            write!(f, "{:.1}M", amount as f64 / 1e6)
        } else if amount >= 1_000_000 {
            write!(
                f,
                "{},{:03},{:03}",
                amount / 1_000_000,
                amount % 1_000_000 / 1_000,
                amount % 1000
            )
        } else if amount >= 1_000 {
            write!(f, "{},{:03}", amount / 1000, amount % 1000)
        } else {
            write!(f, "{}", amount)
        }
    }
}

/// The player cards.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub enum PlayerCards {
    /// The player has no cards.
    #[default]
    None,
    /// The player has cards but their values are covered.
    Covered,
    /// The player cards.
    Cards(Card, Card),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_formatting() {
        assert_eq!(Chips(123).to_string(), "123");
        assert_eq!(Chips(1_000).to_string(), "1,000");
        assert_eq!(Chips(1_234).to_string(), "1,234");
        assert_eq!(Chips(12_345).to_string(), "12,345");
        assert_eq!(Chips(123_456).to_string(), "123,456");
        assert_eq!(Chips(1_000_000).to_string(), "1,000,000");
        assert_eq!(Chips(1_234_567).to_string(), "1,234,567");
        assert_eq!(Chips(10_000_000).to_string(), "10.0M");
        assert_eq!(Chips(123_456_789).to_string(), "123.5M");
    }
}
