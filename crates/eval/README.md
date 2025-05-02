# Freezeout Poker Hands Evaluator

Poker hand evaluator for 5, 6 and 7 cards hands. This evaluator is a port of the
[Cactus Kev's][kevlink] poker evaluator with an additional lookup table for faster 7
cards evaluation (~40M 7-cards hands/s). 

[kevlink]: http://suffe.cool/poker/evaluator.html

To run the [examples](./examples/) use:

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

