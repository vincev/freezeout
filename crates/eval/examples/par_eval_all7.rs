// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
//
// ```bash
// $ cargo r --release --features=eval --example par_eval_all7
// ```

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use freezeout_eval::{deck::*, eval::*};

fn main() {
    // Evaluate all 133M hands with 4 parallel tasks.
    const NUM_TASKS: usize = 4;
    const NUM_RANKS: usize = 9;

    // Create per task counters to avoid contention and boost performance.
    let task_counters = (0..NUM_TASKS)
        .map(|_| {
            (0..NUM_RANKS)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let now = Instant::now();

    Deck::default().par_for_each(NUM_TASKS, 7, |task_id, hand| {
        let rank = HandValue::eval(&hand).rank();
        let counters = &task_counters[task_id];
        counters[rank as usize].fetch_add(1, Ordering::Relaxed);
    });

    let elapsed = now.elapsed().as_secs_f64();

    // Aggregate counters.
    let agg = (0..NUM_RANKS)
        .map(|r| {
            task_counters
                .iter()
                .map(|counts| counts[r].load(Ordering::Relaxed))
                .sum()
        })
        .collect::<Vec<_>>();

    let total = agg.iter().sum::<u64>();
    println!("Total hands      {total}");
    println!("Elapsed:         {:.3}s", elapsed);
    println!("Hands/sec:       {:.0}\n", total as f64 / elapsed);

    println!("High Card:       {}", agg[HandRank::HighCard as usize]);
    println!("One  Pair:       {}", agg[HandRank::OnePair as usize]);
    println!("Two Pairs:       {}", agg[HandRank::TwoPair as usize]);
    println!("Three of a Kind: {}", agg[HandRank::ThreeOfAKind as usize]);
    println!("Staight:         {}", agg[HandRank::Straight as usize]);
    println!("Flush:           {}", agg[HandRank::Flush as usize]);
    println!("Full House:      {}", agg[HandRank::FullHouse as usize]);
    println!("Four of a Kind:  {}", agg[HandRank::FourOfAKind as usize]);
    println!("Straight Flush:  {}", agg[HandRank::StraightFlush as usize]);
}
