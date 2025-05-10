// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
//
// ```bash
// $ cargo r --release --features=parallel --example chart
// ```
use clap::{Parser, value_parser};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use freezeout_eval::*;

#[derive(Default)]
struct Counter {
    wins: AtomicU64,
    games: AtomicU64,
}

impl Counter {
    fn inc_win(&self) {
        self.wins.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_game(&self) {
        self.games.fetch_add(1, Ordering::Relaxed);
    }

    fn wins(&self) -> u64 {
        self.wins.load(Ordering::Relaxed)
    }

    fn games(&self) -> u64 {
        self.games.load(Ordering::Relaxed)
    }
}

fn run_sim(c1: Card, c2: Card, n_against: usize) -> f64 {
    const NUM_TASKS: usize = 4;
    const SAMPLES_PER_TASK: usize = 25_000;
    const HAND_SIZE: usize = 7;
    const BOARD_SIZE: usize = 5;

    assert_ne!(c1, c2);
    assert!(n_against > 0 && n_against < 7);

    // Create per task counters to avoid contention and boost performance.
    let task_counters = (0..NUM_TASKS)
        .map(|_| Counter::default())
        .collect::<Vec<_>>();

    // Remove cards from the deck so that we don't sample them.
    let mut deck = Deck::default();
    deck.remove(c1);
    deck.remove(c2);

    // Two cards for each opponent player plus the board.
    let sample_size = n_against * 2 + BOARD_SIZE;

    deck.par_sample(
        NUM_TASKS,
        SAMPLES_PER_TASK,
        sample_size,
        |task_id, sample| {
            // The sample contains the two cards for each player and the board cards,
            // copy the board cards to the end of the evaluation array.
            let mut hand = [Card::default(); HAND_SIZE];
            let board_start = n_against * 2;
            hand[2..].copy_from_slice(&sample[board_start..]);

            // Evaluate hero hand.
            hand[0] = c1;
            hand[1] = c2;
            let hvalue = HandValue::eval(&hand);

            // Compare against other players hand.
            let mut has_lost = false;
            for player in 0..n_against {
                hand[0] = sample[player * 2];
                hand[1] = sample[player * 2 + 1];
                let ovalue = HandValue::eval(&hand);
                if ovalue > hvalue {
                    has_lost = true;
                    break;
                }
            }

            let counter = &task_counters[task_id];
            if !has_lost {
                counter.inc_win();
            }

            counter.inc_game();
        },
    );

    // Aggregate counters.
    let wins = task_counters.iter().map(|c| c.wins()).sum::<u64>();
    let total = task_counters.iter().map(|c| c.games()).sum::<u64>();
    (wins as f64 / total as f64) * 100.0
}

fn separator() {
    print!("|");
    for _ in 0..13 {
        print!("-----|");
    }
    println!();
}

#[derive(Debug, Parser)]
struct Cli {
    /// The number of opposing players.
    #[clap(long, short, default_value_t = 1, value_parser = value_parser!(u8).range(1..=6))]
    num_players: u8,
}

fn main() {
    let cli = Cli::parse();
    let num_players = cli.num_players as usize;

    separator();

    let now = Instant::now();

    for r1 in Rank::ranks().rev() {
        let mut labels = Vec::with_capacity(13);
        let mut probs = Vec::with_capacity(13);

        for r2 in Rank::ranks().rev() {
            let (c1, c2) = if r1 < r2 || r1 == r2 {
                // Offsuit or pair
                (Card::new(r2, Suit::Hearts), Card::new(r1, Suit::Spades))
            } else {
                // Suited cards
                (Card::new(r1, Suit::Hearts), Card::new(r2, Suit::Hearts))
            };

            if c1.rank() == c2.rank() {
                labels.push(format!("{}{} ", c1.rank(), c2.rank()));
            } else if c1.suit() == c2.suit() {
                labels.push(format!("{}{}s", c1.rank(), c2.rank()));
            } else {
                labels.push(format!("{}{}o", c1.rank(), c2.rank()));
            }

            probs.push(run_sim(c1, c2, num_players).round());
        }

        print!("|");
        for label in labels {
            print!(" {label} |");
        }

        println!();

        print!("|");
        for prob in &probs {
            print!(" {:2.0}% |", prob.ceil());
        }
        println!();

        separator();
    }

    println!("Elapsed: {:.3}s", now.elapsed().as_secs_f64());
}
