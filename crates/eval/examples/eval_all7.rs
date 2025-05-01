// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
//
// Run with:
//
// ```bash
// $ cargo r --release --features=eval --example eval_all7
// ...
// Total hands      133784560
// Elapsed:         3.513s
// Hands/sec:       38084445
//
// High Card:       23294460
// One  Pair:       58627800
// Two Pairs:       31433400
// Three of a Kind: 6461620
// Staight:         6180020
// Flush:           4047644
// Full House:      3473184
// Four of a Kind:  224848
// Straight Flush:  41584
// ```

use std::time::Instant;

use freezeout_eval::{cards::*, eval::*};

#[rustfmt::skip]
fn main() {
    // Evaluate all 133M hands.
    let now = Instant::now();
    let mut counts = [0usize; 9];

    Deck::default().for_each(7, |hand| {
        let rank = HandValue::eval(&hand).rank();
        counts[rank as usize] += 1;
    });

    let elapsed = now.elapsed().as_secs_f64();
    let total = counts.iter().sum::<usize>();
    println!("Total hands      {total}");
    println!("Elapsed:         {:.3}s", elapsed);
    println!("Hands/sec:       {:.0}\n", total as f64 / elapsed);

    println!("High Card:       {}", counts[HandRank::HighCard as usize]);
    println!("One  Pair:       {}", counts[HandRank::OnePair as usize]);
    println!("Two Pairs:       {}", counts[HandRank::TwoPair as usize]);
    println!("Three of a Kind: {}", counts[HandRank::ThreeOfAKind as usize]);
    println!("Staight:         {}", counts[HandRank::Straight as usize]);
    println!("Flush:           {}", counts[HandRank::Flush as usize]);
    println!("Full House:      {}", counts[HandRank::FullHouse as usize]);
    println!("Four of a Kind:  {}", counts[HandRank::FourOfAKind as usize]);
    println!("Straight Flush:  {}", counts[HandRank::StraightFlush as usize]);
}
