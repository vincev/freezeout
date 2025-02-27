// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Poker cards definitions.
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Primes used to encode a card rank.
const PRIMES: [u32; 13] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41];

/// A Poker card.
///
/// A card is represented using the encoding in the [Cactus Kev's][kevlink] Poker
/// hand evaluator with each card having the following format:
///
/// ```text
///   +--------+--------+--------+--------+
///   |xxxbbbbb|bbbbbbbb|cdhsrrrr|xxpppppp|
///   +--------+--------+--------+--------+
///   p = prime number of rank (deuce=2,trey=3,four=5,five=7,...,ace=41)
///   r = rank of card (deuce=0,trey=1,four=2,five=3,...,ace=12)
///   cdhs = suit of card
///   b = bit turned on depending on rank of card
/// ```
///
/// [kevlink]: http://suffe.cool/poker/evaluator.html
#[derive(Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Card {
    card_id: u32,
}

/// A Poker card.
impl Card {
    /// Create a card given a suit and rank.
    pub fn new(rank: Rank, suit: Suit) -> Card {
        let (rank, suit) = (rank as u32, suit as u32);
        Card {
            card_id: PRIMES[rank as usize] | (rank << 8) | (suit << 12) | (1 << (rank + 16)),
        }
    }

    /// This card unique id.
    pub fn id(&self) -> u32 {
        self.card_id
    }

    /// Returns the card suit.
    pub fn suit(&self) -> Suit {
        let suit_bits = (self.card_id >> 12) & 0xF;
        match suit_bits {
            0x8 => Suit::Clubs,
            0x4 => Suit::Diamonds,
            0x2 => Suit::Hearts,
            0x1 => Suit::Spades,
            _ => panic!("Invalid suit value 0x{:x}", self.card_id),
        }
    }

    /// Returns the card rank.
    pub fn rank(&self) -> Rank {
        let rank_bits = (self.card_id >> 8) & 0xF;
        match rank_bits {
            0 => Rank::Deuce,
            1 => Rank::Trey,
            2 => Rank::Four,
            3 => Rank::Five,
            4 => Rank::Six,
            5 => Rank::Seven,
            6 => Rank::Eight,
            7 => Rank::Nine,
            8 => Rank::Ten,
            9 => Rank::Jack,
            10 => Rank::Queen,
            11 => Rank::King,
            12 => Rank::Ace,
            _ => panic!("Invalid rank 0x{:x}", self.card_id),
        }
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank(), self.suit())
    }
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Card({}{})", self.rank(), self.suit())
    }
}

/// Card rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// Deuce
    Deuce = 0,
    /// Trey
    Trey,
    /// Four
    Four,
    /// Five
    Five,
    /// Six
    Six,
    /// Seven
    Seven,
    /// Eight
    Eight,
    /// Nine
    Nine,
    /// Ten
    Ten,
    /// Jack
    Jack,
    /// Queen
    Queen,
    /// King
    King,
    /// Ace
    Ace,
}

impl Rank {
    /// Returns all ranks.
    pub fn ranks() -> impl DoubleEndedIterator<Item = Rank> {
        use Rank::*;
        [
            Deuce, Trey, Four, Five, Six, Seven, Eight, Nine, Ten, Jack, Queen, King, Ace,
        ]
        .into_iter()
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rank = match self {
            Rank::Deuce => '2',
            Rank::Trey => '3',
            Rank::Four => '4',
            Rank::Five => '5',
            Rank::Six => '6',
            Rank::Seven => '7',
            Rank::Eight => '8',
            Rank::Nine => '9',
            Rank::Ten => 'T',
            Rank::Jack => 'J',
            Rank::Queen => 'Q',
            Rank::King => 'K',
            Rank::Ace => 'A',
        };

        write!(f, "{rank}")
    }
}

/// Card suit.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suit {
    /// Clubs suit.
    Clubs = 8,
    /// Diamonds suit.
    Diamonds = 4,
    /// Hearts suit.
    Hearts = 2,
    /// Spades suit.
    Spades = 1,
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suit = match self {
            Suit::Clubs => 'C',
            Suit::Diamonds => 'D',
            Suit::Hearts => 'H',
            Suit::Spades => 'S',
        };

        write!(f, "{suit}")
    }
}

impl Suit {
    /// Returns all suits.
    pub fn suits() -> impl DoubleEndedIterator<Item = Suit> {
        [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades].into_iter()
    }
}

/// A cards Deck
#[derive(Debug)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    /// The number of cards in the deck.
    pub const SIZE: usize = 52;

    /// Creates a new shuffled deck.
    pub fn new_and_shuffled<R: Rng>(rng: &mut R) -> Self {
        let mut cards = Suit::suits()
            .flat_map(|s| Rank::ranks().map(move |r| Card::new(r, s)))
            .collect::<Vec<_>>();
        cards.shuffle(rng);
        Self { cards }
    }

    /// Deals a card from the deck.
    pub fn deal(&mut self) -> Card {
        self.cards.pop().unwrap()
    }

    /// Checks if the deck is empty.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

impl IntoIterator for Deck {
    type Item = Card;
    type IntoIter = std::vec::IntoIter<Card>;

    fn into_iter(self) -> Self::IntoIter {
        self.cards.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::HashSet;

    #[test]
    fn card_encoding() {
        let mut cards = HashSet::default();
        let mut deck = Deck::new_and_shuffled(&mut thread_rng());

        while !deck.is_empty() {
            let card = deck.deal();
            assert_eq!(card.id() & 0xFF, PRIMES[card.rank() as usize]);
            assert_eq!(card.id() >> 8 & 0xF, card.rank() as u32);
            assert_eq!(card.id() >> 12 & 0xF, card.suit() as u32);
            assert_eq!(card.id() >> 16, 1 << (card.rank() as usize));
            cards.insert(card.id());
        }

        // Check uniquness.
        assert_eq!(cards.len(), Deck::SIZE);

        // From the Cactus Kev's website.
        let kd = Card::new(Rank::King, Suit::Diamonds);
        assert_eq!(kd.id(), 0x08004b25);

        let fs = Card::new(Rank::Five, Suit::Spades);
        assert_eq!(fs.id(), 0x00081307);

        let jc = Card::new(Rank::Jack, Suit::Clubs);
        assert_eq!(jc.id(), 0x0200891d);
    }

    #[test]
    fn card_to_string() {
        let c = Card::new(Rank::King, Suit::Diamonds);
        assert_eq!(c.to_string(), "KD");

        let c = Card::new(Rank::Five, Suit::Spades);
        assert_eq!(c.to_string(), "5S");

        let c = Card::new(Rank::Jack, Suit::Clubs);
        assert_eq!(c.to_string(), "JC");

        let c = Card::new(Rank::Ten, Suit::Hearts);
        assert_eq!(c.to_string(), "TH");

        let c = Card::new(Rank::Ace, Suit::Hearts);
        assert_eq!(c.to_string(), "AH");
    }
}
