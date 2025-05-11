# Freezeout Poker Cards

This crate implements types for poker cards and provides functionality for
dealing, sampling, and iterating over cards in a deck.

With the **egui** feature enabled it exports texture types to paint cards in an egui
application.

See the [board](./examples/board.rs) example for a simple egui app that uses the
textures:

```bash
$ cargo r --release --features=egui --example board
```

<p align="center">
  <img alt="Board Example" src="../../media/board.png" height="672" width="600">
</p>

