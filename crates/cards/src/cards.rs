// Copyright (C) 2025  Vince Vasta.
// SPDX-License-Identifier: Apache-2.0

//! Cards images loading and painting.
use ahash::AHashMap;
use eframe::egui;
use std::sync::LazyLock;

use crate::{Card, Deck, Rank, Suit};

/// The clubs.
const BYTES_2C: &[u8] = include_bytes!("assets/2c.png");
const BYTES_3C: &[u8] = include_bytes!("assets/3c.png");
const BYTES_4C: &[u8] = include_bytes!("assets/4c.png");
const BYTES_5C: &[u8] = include_bytes!("assets/5c.png");
const BYTES_6C: &[u8] = include_bytes!("assets/6c.png");
const BYTES_7C: &[u8] = include_bytes!("assets/7c.png");
const BYTES_8C: &[u8] = include_bytes!("assets/8c.png");
const BYTES_9C: &[u8] = include_bytes!("assets/9c.png");
const BYTES_TC: &[u8] = include_bytes!("assets/tc.png");
const BYTES_JC: &[u8] = include_bytes!("assets/jc.png");
const BYTES_KC: &[u8] = include_bytes!("assets/kc.png");
const BYTES_QC: &[u8] = include_bytes!("assets/qc.png");
const BYTES_AC: &[u8] = include_bytes!("assets/ac.png");

/// The diamonds.
const BYTES_2D: &[u8] = include_bytes!("assets/2d.png");
const BYTES_3D: &[u8] = include_bytes!("assets/3d.png");
const BYTES_4D: &[u8] = include_bytes!("assets/4d.png");
const BYTES_5D: &[u8] = include_bytes!("assets/5d.png");
const BYTES_6D: &[u8] = include_bytes!("assets/6d.png");
const BYTES_7D: &[u8] = include_bytes!("assets/7d.png");
const BYTES_8D: &[u8] = include_bytes!("assets/8d.png");
const BYTES_9D: &[u8] = include_bytes!("assets/9d.png");
const BYTES_TD: &[u8] = include_bytes!("assets/td.png");
const BYTES_JD: &[u8] = include_bytes!("assets/jd.png");
const BYTES_KD: &[u8] = include_bytes!("assets/kd.png");
const BYTES_QD: &[u8] = include_bytes!("assets/qd.png");
const BYTES_AD: &[u8] = include_bytes!("assets/ad.png");

/// The hearts.
const BYTES_2H: &[u8] = include_bytes!("assets/2h.png");
const BYTES_3H: &[u8] = include_bytes!("assets/3h.png");
const BYTES_4H: &[u8] = include_bytes!("assets/4h.png");
const BYTES_5H: &[u8] = include_bytes!("assets/5h.png");
const BYTES_6H: &[u8] = include_bytes!("assets/6h.png");
const BYTES_7H: &[u8] = include_bytes!("assets/7h.png");
const BYTES_8H: &[u8] = include_bytes!("assets/8h.png");
const BYTES_9H: &[u8] = include_bytes!("assets/9h.png");
const BYTES_TH: &[u8] = include_bytes!("assets/th.png");
const BYTES_JH: &[u8] = include_bytes!("assets/jh.png");
const BYTES_KH: &[u8] = include_bytes!("assets/kh.png");
const BYTES_QH: &[u8] = include_bytes!("assets/qh.png");
const BYTES_AH: &[u8] = include_bytes!("assets/ah.png");

/// The spades.
const BYTES_2S: &[u8] = include_bytes!("assets/2s.png");
const BYTES_3S: &[u8] = include_bytes!("assets/3s.png");
const BYTES_4S: &[u8] = include_bytes!("assets/4s.png");
const BYTES_5S: &[u8] = include_bytes!("assets/5s.png");
const BYTES_6S: &[u8] = include_bytes!("assets/6s.png");
const BYTES_7S: &[u8] = include_bytes!("assets/7s.png");
const BYTES_8S: &[u8] = include_bytes!("assets/8s.png");
const BYTES_TS: &[u8] = include_bytes!("assets/ts.png");
const BYTES_9S: &[u8] = include_bytes!("assets/9s.png");
const BYTES_JS: &[u8] = include_bytes!("assets/js.png");
const BYTES_KS: &[u8] = include_bytes!("assets/ks.png");
const BYTES_QS: &[u8] = include_bytes!("assets/qs.png");
const BYTES_AS: &[u8] = include_bytes!("assets/as.png");

/// The cards back.
const BYTES_BB: &[u8] = include_bytes!("assets/bb.png");

static CARD_IMAGES: LazyLock<AHashMap<Card, &'static [u8]>> = LazyLock::new(|| {
    let mut cards = AHashMap::with_capacity(Deck::SIZE);

    cards.insert(Card::new(Rank::Deuce, Suit::Clubs), BYTES_2C);
    cards.insert(Card::new(Rank::Trey, Suit::Clubs), BYTES_3C);
    cards.insert(Card::new(Rank::Four, Suit::Clubs), BYTES_4C);
    cards.insert(Card::new(Rank::Five, Suit::Clubs), BYTES_5C);
    cards.insert(Card::new(Rank::Six, Suit::Clubs), BYTES_6C);
    cards.insert(Card::new(Rank::Seven, Suit::Clubs), BYTES_7C);
    cards.insert(Card::new(Rank::Eight, Suit::Clubs), BYTES_8C);
    cards.insert(Card::new(Rank::Nine, Suit::Clubs), BYTES_9C);
    cards.insert(Card::new(Rank::Ten, Suit::Clubs), BYTES_TC);
    cards.insert(Card::new(Rank::Jack, Suit::Clubs), BYTES_JC);
    cards.insert(Card::new(Rank::Queen, Suit::Clubs), BYTES_QC);
    cards.insert(Card::new(Rank::King, Suit::Clubs), BYTES_KC);
    cards.insert(Card::new(Rank::Ace, Suit::Clubs), BYTES_AC);

    cards.insert(Card::new(Rank::Deuce, Suit::Diamonds), BYTES_2D);
    cards.insert(Card::new(Rank::Trey, Suit::Diamonds), BYTES_3D);
    cards.insert(Card::new(Rank::Four, Suit::Diamonds), BYTES_4D);
    cards.insert(Card::new(Rank::Five, Suit::Diamonds), BYTES_5D);
    cards.insert(Card::new(Rank::Six, Suit::Diamonds), BYTES_6D);
    cards.insert(Card::new(Rank::Seven, Suit::Diamonds), BYTES_7D);
    cards.insert(Card::new(Rank::Eight, Suit::Diamonds), BYTES_8D);
    cards.insert(Card::new(Rank::Nine, Suit::Diamonds), BYTES_9D);
    cards.insert(Card::new(Rank::Ten, Suit::Diamonds), BYTES_TD);
    cards.insert(Card::new(Rank::Jack, Suit::Diamonds), BYTES_JD);
    cards.insert(Card::new(Rank::Queen, Suit::Diamonds), BYTES_QD);
    cards.insert(Card::new(Rank::King, Suit::Diamonds), BYTES_KD);
    cards.insert(Card::new(Rank::Ace, Suit::Diamonds), BYTES_AD);

    cards.insert(Card::new(Rank::Deuce, Suit::Hearts), BYTES_2H);
    cards.insert(Card::new(Rank::Trey, Suit::Hearts), BYTES_3H);
    cards.insert(Card::new(Rank::Four, Suit::Hearts), BYTES_4H);
    cards.insert(Card::new(Rank::Five, Suit::Hearts), BYTES_5H);
    cards.insert(Card::new(Rank::Six, Suit::Hearts), BYTES_6H);
    cards.insert(Card::new(Rank::Seven, Suit::Hearts), BYTES_7H);
    cards.insert(Card::new(Rank::Eight, Suit::Hearts), BYTES_8H);
    cards.insert(Card::new(Rank::Nine, Suit::Hearts), BYTES_9H);
    cards.insert(Card::new(Rank::Ten, Suit::Hearts), BYTES_TH);
    cards.insert(Card::new(Rank::Jack, Suit::Hearts), BYTES_JH);
    cards.insert(Card::new(Rank::Queen, Suit::Hearts), BYTES_QH);
    cards.insert(Card::new(Rank::King, Suit::Hearts), BYTES_KH);
    cards.insert(Card::new(Rank::Ace, Suit::Hearts), BYTES_AH);

    cards.insert(Card::new(Rank::Deuce, Suit::Spades), BYTES_2S);
    cards.insert(Card::new(Rank::Trey, Suit::Spades), BYTES_3S);
    cards.insert(Card::new(Rank::Four, Suit::Spades), BYTES_4S);
    cards.insert(Card::new(Rank::Five, Suit::Spades), BYTES_5S);
    cards.insert(Card::new(Rank::Six, Suit::Spades), BYTES_6S);
    cards.insert(Card::new(Rank::Seven, Suit::Spades), BYTES_7S);
    cards.insert(Card::new(Rank::Eight, Suit::Spades), BYTES_8S);
    cards.insert(Card::new(Rank::Nine, Suit::Spades), BYTES_9S);
    cards.insert(Card::new(Rank::Ten, Suit::Spades), BYTES_TS);
    cards.insert(Card::new(Rank::Jack, Suit::Spades), BYTES_JS);
    cards.insert(Card::new(Rank::Queen, Suit::Spades), BYTES_QS);
    cards.insert(Card::new(Rank::King, Suit::Spades), BYTES_KS);
    cards.insert(Card::new(Rank::Ace, Suit::Spades), BYTES_AS);

    cards
});

/// A collection of cards textures used for drawing.
pub struct Textures {
    cards: AHashMap<Card, egui::TextureHandle>,
    back: egui::TextureHandle,
}

impl Textures {
    /// Loads the cards textures.
    pub fn new(ctx: &egui::Context) -> Self {
        let cards = CARD_IMAGES
            .iter()
            .map(|(card, image_data)| {
                (
                    *card,
                    ctx.load_texture(
                        card.to_string(),
                        image_from_memory(image_data),
                        Default::default(),
                    ),
                )
            })
            .collect();

        let back = ctx.load_texture("back", image_from_memory(BYTES_BB), Default::default());

        Self { cards, back }
    }

    /// Gets a texture for a card.
    pub fn card(&self, card: Card) -> egui::TextureHandle {
        self.cards.get(&card).unwrap().clone()
    }

    /// Gets a texture for a hole card.
    pub fn back(&self) -> egui::TextureHandle {
        self.back.clone()
    }
}

fn image_from_memory(image_data: &[u8]) -> egui::ColorImage {
    let image = image::load_from_memory(image_data).unwrap();
    let size = [image.width() as _, image.height() as _];
    let image_buffer = image.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice())
}
