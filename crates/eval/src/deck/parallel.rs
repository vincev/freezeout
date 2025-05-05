// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Parallel hand iteration.
use rand::prelude::*;
use std::thread;

use super::{Card, Deck, Rank, Suit};

/// Creates table for nck(n, k) for n <= 52 and k <= 7.
const fn make_nck() -> [[u32; 8]; 52] {
    let mut t = [[0u32; 8]; 52];
    let mut n = 0;

    while n < 52 {
        // base case nck(n, 0) = 1
        t[n][0] = 1;

        let mut k = 1;
        while k <= 7 && k <= n + 1 {
            // nck(n, k) = nck(n-1, k-1) + nck(n-1, k)
            let n_1 = n.saturating_sub(1);
            let k_1 = k.saturating_sub(1);
            t[n][k] = t[n_1][k_1] + t[n_1][k];
            k += 1;
        }

        n += 1;
    }

    t
}

const NCKS: [[u32; 8]; 52] = make_nck();

/// Returns the binomial coefficient for n choose k.
#[inline]
fn nck(n: usize, k: usize) -> usize {
    assert!(n <= 52, "n={n} must be 0 <= n <= 52");
    assert!(k <= 7, "k={k} must be 0 <= k <= 7");

    if n < k || n == 0 {
        0
    } else {
        NCKS[n.saturating_sub(1)][k] as usize
    }
}

/// Uses the combinatorial number system to convert n to a
/// k-combination (see Theorem L pg. 260 Knuth 4a).
fn nth_ksubset(mut n: usize, k: usize) -> [usize; 7] {
    assert!(k <= 7);

    let mut out = [0; 7];
    for k in (0..k).rev() {
        let mut c = k;
        while nck(c, k + 1) <= n {
            c += 1;
        }

        c = c.saturating_sub(1);
        out[k] = c;

        n = n.saturating_sub(nck(c, k + 1) as usize);
    }

    out
}

/// Calls the given closure for count k-subsets starting from the nth ksubset.
fn for_each_ksubset<F>(n: usize, k: usize, nth: usize, count: usize, mut f: F)
where
    F: FnMut(&[usize]),
{
    // Algorithm L from TAOCP 4a
    let mut c = vec![0usize; k + 3];

    let ks = nth_ksubset(nth, k);
    for i in 0..k {
        c[i + 1] = ks[i];
    }

    c[k + 1] = n;

    let mut counter = 1;
    loop {
        f(&c[1..=k]);

        counter += 1;
        if counter > count {
            break;
        }

        let mut j = 1;
        while c[j] + 1 == c[j + 1] {
            c[j] = j - 1;
            j = j + 1;
        }

        if j > k {
            break;
        }

        c[j] += 1;
    }
}

impl Deck {
    /// Parallel for each, calls the `f` closure for each k-cards hand.
    ///
    /// The clousure takes an usize that is the task identifier (0..num_task)
    /// and a slice of cards of length k.
    ///
    /// Panics if k is not 2 <= k <= 7.
    pub fn par_for_each<F>(&self, num_tasks: usize, k: usize, f: F)
    where
        F: Fn(usize, &[Card]) + Send + Sync,
    {
        assert!(2 <= k && k <= 7, "2 <= k <= 7");
        assert!(num_tasks > 0);

        if k > self.cards.len() {
            return;
        }

        let n = self.cards.len();
        let num_hands = nck(n, k);
        let hands_per_task = (num_hands + num_tasks - 1) / num_tasks;

        thread::scope(|s| {
            for task_id in 0..num_tasks {
                let start = task_id * hands_per_task;
                let f = &f;
                s.spawn(move || {
                    let mut h = vec![Card::new(Rank::Ace, Suit::Diamonds); k];
                    for_each_ksubset(n, k, start, hands_per_task, |p| {
                        for (idx, &pos) in p.iter().enumerate() {
                            h[idx] = self.cards[pos];
                        }

                        f(task_id, &h);
                    });
                });
            }
        });
    }

    /// Calls the given closure from `num_tasks` parallel tasks generating
    /// `samples_per_task` samples of size k.
    pub fn par_sample<F>(&self, num_tasks: usize, samples_per_task: usize, k: usize, f: F)
    where
        F: Fn(usize, &[Card]) + Send + Sync,
    {
        assert!(2 <= k && k <= 7, "2 <= k <= 7");
        assert!(num_tasks > 0);
        assert!(samples_per_task > 0);

        if k > self.cards.len() {
            return;
        }

        thread::scope(|s| {
            for task_id in 0..num_tasks {
                let f = &f;
                s.spawn(move || {
                    let mut h = vec![Card::new(Rank::Ace, Suit::Diamonds); k];
                    let mut rng = SmallRng::from_os_rng();

                    for _ in 0..samples_per_task {
                        for (pos, c) in self.cards.choose_multiple(&mut rng, k).enumerate() {
                            h[pos] = *c;
                        }

                        f(task_id, &h);
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nck() {
        // For n < k = 0
        assert_eq!(nck(2, 3), 0);

        [1, 52, 1326, 22100, 270725, 2598960, 20358520, 133784560]
            .into_iter()
            .enumerate()
            .for_each(|(k, v)| assert_eq!(nck(52, k), v));

        [1, 51, 1275, 20825, 249900, 2349060, 18009460, 115775100]
            .into_iter()
            .enumerate()
            .for_each(|(k, v)| assert_eq!(nck(51, k), v));

        [1, 23, 253, 1771, 8855, 33649, 100947, 245157]
            .into_iter()
            .enumerate()
            .for_each(|(k, v)| assert_eq!(nck(23, k), v));

        [1, 5, 10, 10, 5, 1, 0, 0]
            .into_iter()
            .enumerate()
            .for_each(|(k, v)| assert_eq!(nck(5, k), v));

        [1, 1, 0, 0, 0, 0, 0, 0]
            .into_iter()
            .enumerate()
            .for_each(|(k, v)| assert_eq!(nck(1, k), v));
    }

    // This takes a while to run in debug mode as it goes through 200M subsets.
    #[test]
    #[ignore]
    fn test_nth_ksubset() {
        let mut counter = 0;
        let count = nck(52, 7);
        for_each_ksubset(52, 7, 0, count, |s| {
            let ks = nth_ksubset(counter, 7);
            s.iter().zip(ks).for_each(|(&l, r)| assert_eq!(l, r));
            counter += 1;
        });

        assert_eq!(count, counter);

        // Start from half way.
        counter = 0;
        let nth = nck(52, 7) / 2;
        for_each_ksubset(52, 7, nth, nth, |s| {
            let ks = nth_ksubset(nth + counter, 7);
            s.iter().zip(ks).for_each(|(&l, r)| assert_eq!(l, r));
            counter += 1;
        });

        assert_eq!(nth, counter);
    }
}
