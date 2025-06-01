# Freezeout Poker Cards

This crate implements types for poker cards and provides functionality for
dealing, sampling, and iterating over cards in a deck.

The **parallel** feature enables parallel iteration and sampling with the
`Deck::par_for_each` (see the [par_eval_all7](../eval/examples/par_eval_all7.rs)
example) and `Deck::par_sample` (see the [chart](../eval/examples/chart.rs) example)
methods.

The **egui** feature enables the `Textures` type that provides access to cards
textures used to paint cards in an egui application, see the
[board](../eval/examples/board.rs) example in the [eval](../eval/) crate for a simple
egui app that uses the textures.

## Using Freezeout Cards

[freezeout-cards is available on crates.io](https://crates.io/crates/freezeout-cards).
To use it in your project add a dependency to your `Cargo.toml`:

```toml
[dependencies]
freezeout-cards = "0.2.1"
```