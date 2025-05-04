# Freezeout Poker Hands Evaluator

Poker hand evaluator for 5, 6 and 7 cards hands. This evaluator is a port of the
[Cactus Kev's][kevlink] poker evaluator with an additional lookup table for
faster 7 cards evaluation.

On my box I get ~40M 7-cards hands/s with a single thread and around ~115M 7-cards
hands/s with parallel processing (4 tasks).

To run the [single threaded example](./examples/eval_all7.rs):

```bash
$ cargo r --release --features=eval --example eval_all7
...
Total hands      133784560
Elapsed:         3.195s
Hands/sec:       41875270

High Card:       23294460
One  Pair:       58627800
Two Pairs:       31433400
Three of a Kind: 6461620
Staight:         6180020
Flush:           4047644
Full House:      3473184
Four of a Kind:  224848
Straight Flush:  41584
```

To run the [multi threaded example](./examples/par_eval_all7.rs)::

```bash
cargo r --release --features=eval --example par_eval_all7
...
Total hands      133784560
Elapsed:         1.151s
Hands/sec:       116270956

High Card:       23294460
One  Pair:       58627800
Two Pairs:       31433400
Three of a Kind: 6461620
Staight:         6180020
Flush:           4047644
Full House:      3473184
Four of a Kind:  224848
Straight Flush:  41584
```

[kevlink]: http://suffe.cool/poker/evaluator.html
