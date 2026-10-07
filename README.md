# Hangman agent

The agent names one letter at a time and identifies the secret. The secret is any word in `data/words.txt`: 393,972 words, the union of the `words_alpha` list, the public-domain ENABLE Scrabble lexicon, and the system dictionary. One-letter entries are only **a**, **i**, and **o**. The other twenty-three letters are not words, and counting them would turn the game into an alphabet search.

Length is visible, because the board shows it. A guess is one unused letter. The reply is the set of positions where that letter sits, or a miss. The agent wins when one lexicon word remains.

It was played against all 393,972 words. It is wrong at most **23** times, and 23 is forced, so no strategy has a smaller worst case on this list. With 23 misses allowed, it does not lose. With 6 misses allowed, this policy loses 17,892 words. No policy can bring that to zero: the four-letter family already requires 17 misses.

## Proof

`cargo run --release -- verify` walks the decision tree once per length and records every word. The hardest word of each length is then played a second time, one guess after another, and the miss counts have to match. `results/proof.txt` is that run.

The bound in the table is not a heuristic. Take every set of words that are identical except in one square, and whose letters in that square do not appear in the shared squares. Guessing one of those letters rules out one word when it misses. Some word in a family of size K therefore costs K − 1 misses under every policy. The largest such family sets the bound.

Two-letter words ending in **s** are the largest. There are 24 of them. The bound is 23, and **yr** takes 23:

```
s- a- m- o- d- t- l- e- i- c- b- r+ f- g- h- j- k- n- p- q- u- v- w- x-
```

A `+` is a hit and a `-` is a miss. After `x` misses, `yr` is the only word left. The 23rd miss is the guess that identifies it, so the rules have to accept that guess. The agent never makes a 24th.

| Length | Words | Bound | Worst | Gap | Hardest word |
|---:|---:|---:|---:|---:|---|
| 1 | 3 | 2 | 2 | 0 | o |
| 2 | 427 | 23 | 23 | 0 | yr |
| 3 | 2,144 | 21 | 21 | 0 | zat |
| 4 | 7,279 | 17 | 17 | 0 | yell |
| 5 | 16,120 | 15 | 16 | 1 | vizzy |
| 6 | 30,296 | 12 | 14 | 2 | popper |
| 7 | 42,717 | 11 | 12 | 1 | buzzing |
| 8 | 52,717 | 8 | 9 | 1 | dungiest |
| 9 | 54,728 | 7 | 8 | 1 | mummeries |
| 10 | 49,814 | 4 | 6 | 2 | bocklogged |
| 11 | 41,439 | 5 | 6 | 1 | bullishness |
| 12 | 32,572 | 3 | 4 | 1 | amphicarpium |
| 13 | 23,628 | 2 | 4 | 2 | antambulacral |
| 14 | 16,242 | 2 | 3 | 1 | chromatography |
| 15 | 10,335 | 1 | 3 | 2 | subquadrangular |
| 16 | 6,229 | 2 | 3 | 1 | chondrodystrophy |
| 17 | 3,610 | 1 | 3 | 2 | ultrastructurally |
| 18 | 1,857 | 1 | 2 | 1 | anthropomorphology |
| 19 | 971 | 1 | 2 | 1 | chlorofluorocarbons |
| 20 | 465 | 1 | 1 | 0 | anthropomorphization |
| 21 | 207 | 0 | 2 | 2 | humuhumunukunukuapuaa |
| 22 | 94 | 1 | 1 | 0 | disestablishmentarians |
| 23 | 40 | 1 | 1 | 0 | dihdroxycholecalciferol |
| 24 | 19 | 0 | 1 | 1 | pathologicopsychological |
| 25 | 9 | 0 | 1 | 1 | philosophicopsychological |
| 27 | 4 | 0 | 0 | 0 | electroencephalographically |
| 28 | 3 | 0 | 0 | 0 | antidisestablishmentarianism |
| 29 | 2 | 0 | 0 | 0 | cyclotrimethylenetrinitramine |
| 31 | 1 | 0 | 0 | 0 | dichlorodiphenyltrichloroethane |

Where the gap is 0, the policy meets the family bound and is optimal for that length. Where the gap is 1 or 2, the family bound does not pin the exact number; the worst column is the number this policy actually used, and every word of that length finished at or under it. The lexicon's worst case is length 2, and that one is pinned at 23.

Misses allowed, and how many words would be lost:

| Misses allowed | Words lost |
|---:|---:|
| 6 | 17,892 |
| 8 | 7,621 |
| 10 | 3,029 |
| 23 | 0 |

## How a letter is chosen

The remaining words are an explicit set. Each unused letter partitions that set by the bitmask of positions where it appears. The empty mask is a miss and costs one. Any other mask is free.

For a large set the agent scores a letter by the worst child of `miss + family bound`, and breaks ties toward the smaller miss pile, then the smaller largest pile, then the letter earlier in the alphabet. For a set of 16 or fewer words it switches to minimax: it tries the legal letters and keeps the one whose deepest branch is cheapest, and it stops a branch as soon as that branch cannot beat the best letter already found.

A network is the wrong instrument for this. The state is a set of strings, the legal moves are 26 letters, and the certificate is the enumeration above. A weight file cannot produce that certificate.

## Play

```bash
cargo run --release -- show yr
cargo run --release -- play
cargo run --release -- verify
```

`show` prints the guesses for a lexicon word. `play` asks for the length, names a letter, and reads the board back (`____` or `_a__`). `verify` is the full proof.

The same policy is in `web/agent.js`. From this directory:

```bash
python3 -m http.server 8765
```

Open `http://127.0.0.1:8765/web/`. Type a lexicon word and it plays the line. `node web/check.mjs yr jazz yell` checks the page's agent against `show`.

`data/words.txt` is sorted, one lowercase word per line. SHA-256: `f92897dbdc808be1528dad66cc93e1a679a94483c09608c8483c672fc55562dd`.
