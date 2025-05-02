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
pub struct Card(u32);

/// A Poker card.
impl Card {
    /// Create a card given a suit and rank.
    pub fn new(rank: Rank, suit: Suit) -> Card {
        let (rank, suit) = (rank as u32, suit as u32);
        Self(PRIMES[rank as usize] | (rank << 8) | (suit << 12) | (1 << (rank + 16)))
    }

    /// This card unique id.
    pub fn id(&self) -> u32 {
        self.0
    }

    /// Returns the card suit.
    pub fn suit(&self) -> Suit {
        let suit_bits = self.suit_bits();
        match suit_bits {
            0x8 => Suit::Clubs,
            0x4 => Suit::Diamonds,
            0x2 => Suit::Hearts,
            0x1 => Suit::Spades,
            _ => panic!("Invalid suit value 0x{:x}", self.0),
        }
    }

    /// Returns the card rank.
    pub fn rank(&self) -> Rank {
        let rank_bits = self.rank_bits();
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
            _ => panic!("Invalid rank 0x{:x}", self.0),
        }
    }

    /// Returns the rank bits.
    #[inline]
    pub fn rank_bits(&self) -> u8 {
        ((self.0 >> 8) & 0xf) as u8
    }

    /// Returns the suit bits.
    #[inline]
    pub fn suit_bits(&self) -> u8 {
        ((self.0 >> 12) & 0xf) as u8
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
        let mut deck = Self::default();
        deck.cards.shuffle(rng);
        deck
    }

    /// Deals a card from the deck.
    pub fn deal(&mut self) -> Card {
        self.cards.pop().unwrap()
    }

    /// Checks if the deck is empty.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Number of cards in the deck.
    pub fn count(&self) -> usize {
        self.cards.len()
    }

    /// Removes a card from the deck.
    pub fn remove(&mut self, card: Card) {
        self.cards.retain(|c| c != &card);
    }

    /// Calls the `f` closure for each k-cards hand.
    ///
    /// Panics if k is not 2 <= k <= 7.
    pub fn for_each<F>(&self, k: usize, mut f: F)
    where
        F: FnMut(&[Card]),
    {
        assert!(2 <= k && k <= 7, "2 <= k <= 7");

        if k > self.cards.len() {
            return;
        }

        let n = self.cards.len();
        let mut h = vec![Card::new(Rank::Ace, Suit::Hearts); 7];

        for c1 in 0..n {
            h[0] = self.cards[c1];

            for c2 in (c1 + 1)..n {
                h[1] = self.cards[c2];

                if k == 2 {
                    f(&h[0..k]);
                    continue;
                }

                for c3 in (c2 + 1)..n {
                    h[2] = self.cards[c3];

                    if k == 3 {
                        f(&h[0..k]);
                        continue;
                    }

                    for c4 in (c3 + 1)..n {
                        h[3] = self.cards[c4];

                        if k == 4 {
                            f(&h[0..k]);
                            continue;
                        }

                        for c5 in (c4 + 1)..n {
                            h[4] = self.cards[c5];

                            if k == 5 {
                                f(&h[0..k]);
                                continue;
                            }

                            for c6 in (c5 + 1)..n {
                                h[5] = self.cards[c6];

                                if k == 6 {
                                    f(&h[0..k]);
                                    continue;
                                }

                                for c7 in (c6 + 1)..n {
                                    h[6] = self.cards[c7];
                                    f(&h[0..k]);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Default for Deck {
    fn default() -> Self {
        let cards = Suit::suits()
            .flat_map(|s| Rank::ranks().map(move |r| Card::new(r, s)))
            .collect::<Vec<_>>();
        Self { cards }
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
        let mut deck = Deck::new_and_shuffled(&mut rand::rng());

        while !deck.is_empty() {
            let card = deck.deal();
            assert_eq!(card.id() & 0xFF, PRIMES[card.rank() as usize]);
            assert_eq!((card.id() >> 8) & 0xF, card.rank() as u32);
            assert_eq!((card.id() >> 12) & 0xF, card.suit() as u32);
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

    #[test]
    fn deck_for_each() {
        let deck = Deck::default();
        assert_eq!(deck.count(), Deck::SIZE);

        let mut hands = HashSet::default();
        deck.for_each(5, |cards| {
            assert_eq!(cards.len(), 5);
            hands.insert(cards.to_owned());
        });
        assert_eq!(hands.len(), 2_598_960);

        hands.clear();
        deck.for_each(2, |cards| {
            assert_eq!(cards.len(), 2);
            hands.insert(cards.to_owned());
        });
        assert_eq!(hands.len(), 1_326);

        hands.clear();
        deck.for_each(3, |cards| {
            assert_eq!(cards.len(), 3);
            hands.insert(cards.to_owned());
        });
        assert_eq!(hands.len(), 22_100);
    }

    #[test]
    fn deck_for_each_7cards() {
        let deck = Deck::default();

        let mut count = 0;
        deck.for_each(7, |cards| {
            assert_eq!(cards.len(), 7);
            count += 1;
        });
        assert_eq!(count, 133_784_560);
    }

    #[test]
    fn deck_for_each_remove() {
        let mut deck = Deck::default();
        deck.remove(Card::new(Rank::Ace, Suit::Diamonds));
        deck.remove(Card::new(Rank::King, Suit::Diamonds));

        let mut count = 0;
        deck.for_each(7, |cards| {
            assert_eq!(cards.len(), 7);
            count += 1;
        });
        assert_eq!(count, 99_884_400);
    }
}
