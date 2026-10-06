# Parity scoreboard: the real playthrough on wf-core

Tree: `9a905c7` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
(the cuts: `scoreboard.py cut`, local under `work/real/`; the run itself: `tools/segments/realrun.py`, §"How to replay
the real run" in `engine/docs/segments.md`). A segment = the real game from one importable area entry (an exit
arrival with nothing running) to the next; an entry by a script warp / in a cutscene is not a cut - its frames
stay in the segment before it, and a segment runs on across the boundaries of the run's key logs (`scoreboard.py
extend`). "not cut" = frames in no segment (the run's start up to its first importable entry) - a chapter's
frames are counted where its segments start.

| chapter | owner | run frames | not cut | segments | exact | frames | matched | % |
|---|---|---|---|---|---|---|---|---|
| opening | Earth agent (title, New Game, Other World) | 42,275 | 7,898 | 17 | 17 | 34,377 | 34,377 | 100.0 |
| earth | Earth agent | 160,503 | 2,938 | 90 | 90 | 157,565 | 157,565 | 100.0 |
| ship | ship agent | 398,194 | 2,578 | 338 | 338 | 395,616 | 395,616 | 100.0 |
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,648 | 8,439 | 371 | 371 | 380,209 | 380,209 | 100.0 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 255 | 187,291 | 187,291 | 100.0 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 447 | 467,701 | 443,700 | 94.9 |
| **all** |  | 1,668,052 | 45,293 | 1519 | 1518 | 1,622,759 | 1,598,758 | 98.5 |

**Whole run, no imports, no lag frames (the target: one step per game logic iteration): exact to frame 1,339,017** of 1,668,052 recorded (062_m_inside_buu; first divergence: rng: app [b460ebed, 721626b0] game [699cd8d3, d8087ea9]). Tree `9fb67ca`, `tools/wholerun/board.py`.

**Whole run, no imports, recorded lag injected (alignment check): exact to frame 1,339,017** of 1,668,052 recorded (062_m_inside_buu; first divergence: rng: app [b460ebed, 721626b0] game [699cd8d3, d8087ea9]). Tree `9fb67ca`, `tools/wholerun/board.py`.

## Whole run with resync: every stop, per owner

The app from the reset; at each stop the game's state is cut at the next row and the run goes on (`tools/wholerun/resync.py`). "rule" = the app diverged from a state it had right; "import" = the cut was refused or diverged in its first rows (importer). Each group: count, first stop.

**rec lag:** 20 stops (16 rule, 4 import)

| count | kind | field | chapter | owner | first row | first stop |
|---|---|---|---|---|---|---|
| 11 | rule | rng | superbuu | Super Buu agent | 1,339,017 | app [b460ebed, 721626b0] game [699cd8d3, d8087ea9] |
| 2 | rule | flags | janemba | side-content agent | 1,111,492 | flag 284 app only |
| 1 | rule | flags | fat | dungeon agents A / B | 848,743 | flag 187 app only |
| 1 | rule | pos | fat | dungeon agents A / B | 848,747 | app (30720, 30720, 1) game (204800, 225280, 1) |
| 1 | rule | rng | janemba | side-content agent | 1,121,917 | app [be62dcff, 5b07fbc6] game [e2b8d8a9, 5be649e5] |
| 3 | import | rng | janemba | importer agent | 1,121,864 | app [af435554, 21224f53] game [ccb45104, c0b2bf81] |
| 1 | import | flags | janemba | importer agent | 1,111,493 | flag 284 game only |

**none lag:** 10 stops (5 rule, 4 import)

| count | kind | field | chapter | owner | first row | first stop |
|---|---|---|---|---|---|---|
| 2 | rule | flags | janemba | side-content agent | 1,111,492 | flag 284 app only |
| 1 | rule | flags | fat | dungeon agents A / B | 848,743 | flag 187 app only |
| 1 | rule | pos | fat | dungeon agents A / B | 848,747 | app (30720, 30720, 1) game (204800, 225280, 1) |
| 1 | rule | rng | janemba | side-content agent | 1,121,917 | app [be62dcff, 5b07fbc6] game [e2b8d8a9, 5be649e5] |
| 3 | import | rng | janemba | importer agent | 1,121,864 | app [af435554, 21224f53] game [ccb45104, c0b2bf81] |
| 1 | import | flags | janemba | importer agent | 1,111,493 | flag 284 game only |
| 1 | known | rng | superbuu | Super Buu agent | 1,339,017 | app [b460ebed, 721626b0] game [699cd8d3, d8087ea9] |


## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 0 | 0 | 1 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 1 segments, 24,001 frames lost

* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `0d79e76`, tiles of 1,000 frames; the whole run 96.9% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**superbuu** (Super Buu agent) - 16 divergences inside tiles, 12,485 frames

* `r062_001_20-6` frame 7,027 (position / facing): 973 lost - Some((27, "pos: #2 sprite 280: (234.90625, 266.58203125) vs game (234.20703125, 267.14453125)"))
* `r062_001_20-6` frame 16,031 (position / facing): 969 lost - Some((31, "pos: #2 sprite 280: (171.546875, 241.95703125) vs game (170.3671875, 240.88671875)"))
* `r062_001_20-6` frame 12,035 (position / facing): 965 lost - Some((35, "pos: #2 sprite 280: (164.8828125, 236.5625) vs game (164.359375, 235.86328125)"))
* `r062_001_20-6` frame 9,036 (position / facing): 964 lost - Some((36, "pos: #2 sprite 280: (167.4453125, 270.2734375) vs game (167.1875, 268.875)"))
* `r062_001_20-6` frame 10,085 (position / facing): 915 lost - Some((85, "pos: #2 sprite 280: (181.08984375, 191.49609375) vs game (180.390625, 191.34765625)"))


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth


### ship


### fat


### janemba


### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
