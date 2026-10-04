# Parity scoreboard: the real playthrough on wf-core

Tree: `00e82a5` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
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
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,648 | 8,439 | 371 | 369 | 380,209 | 369,912 | 97.3 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 253 | 187,291 | 186,412 | 99.5 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 446 | 467,701 | 441,978 | 94.5 |
| **all** |  | 1,668,052 | 45,293 | 1519 | 1513 | 1,622,759 | 1,585,860 | 97.7 |

**Whole run, no imports (recorded lag): exact to frame 153,101** of 723,849 recorded (E2_earth; first divergence: rng: app [ddf7897e, 38c3c1f0] game [234beb00, fbe02e0f]). Tree `55228ff`, `tools/wholerun/board.py`.

**Whole run, no imports (sim lag): exact to frame 4,691** of 723,849 recorded (E1_opening; first divergence: area: app (0, 96) game (1, 1)). Tree `55228ff`, `tools/wholerun/board.py`.
The recording follows the real run from the reset through 002_d04_bandana only: replayed from the reset, the key log of 003_d06_thieves leaves the real run (the chain's state at 002's end differs from the local state 003 was packed from - object list, heap, one video frame) and ends in a game over.

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| menu timing | Earth agent (redraw / mixer timing) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| camera | chapter agent (camera scroll) | 0 | 0 | 0 | 1 | 1 | 1 | 3 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 0 | 0 | 1 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 2 segments, 25,723 frames lost

* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r067_071_14-2` (067_k03_goku158, camera): 1,722 lost - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 1 segments, 9,742 frames lost

* `r002_006_4-1` (002_d04_bandana, camera): 9,742 lost - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload model on a 3D map

**Earth agent (redraw / mixer timing)** - 1 segments, 681 frames lost

* `r044_015_18-1` (044_h_back1, menu timing): 681 lost - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

**cutscene / script-host agent** - 1 segments, 555 frames lost

* `r012_014_12-19` (012_db5_a01, screen fade): 555 lost - Some((2567, "fade: black true vs game false"))

**side-content agent** - 1 segments, 198 frames lost

* `r038_001_18-1` (038_j03_goku120, camera): 198 lost - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `00e82a5`, tiles of 1,000 frames; the whole run 96.4% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**earth** (Earth agent) - 2 divergences inside tiles, 1,881 frames

* `rE2__087_2-90` frame 5,051 (sprite memory): 949 lost - Some((51, "vram: used blocks (start, size) ours only [(447, 16)], game only []"))
* `rE2__087_2-90` frame 1,068 (sprite memory): 932 lost - Some((68, "vram: used blocks (start, size) ours only [(439, 16)], game only []"))

**fat** (dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)) - 4 divergences inside tiles, 3,090 frames

* `r002_006_4-1` frame 6,008 (RNG): 992 lost - Some((8, "rng: [0d8cf402, 260df3cf] vs game [089b453c, a09338fc]"))
* `r002_006_4-1` frame 18,014 (animation): 986 lost - Some((14, "anim: #9 sprite 269: raw 5 cel 0 vs game raw 1 cel 0 tick 62"))
* `r002_006_4-1` frame 19,292 (sprite memory): 708 lost - Some((292, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
* `r002_006_4-1` frame 10,596 (camera): 404 lost - Some((623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)"))

**superbuu** (Super Buu agent) - 19 divergences inside tiles, 15,045 frames

* `r062_001_20-6` frame 7,027 (position / facing): 973 lost - Some((27, "pos: #2 sprite 280: (234.90625, 266.58203125) vs game (234.20703125, 267.14453125)"))
* `r062_001_20-6` frame 16,031 (position / facing): 969 lost - Some((31, "pos: #2 sprite 280: (171.546875, 241.95703125) vs game (170.3671875, 240.88671875)"))
* `r062_001_20-6` frame 12,035 (position / facing): 965 lost - Some((35, "pos: #2 sprite 280: (164.8828125, 236.5625) vs game (164.359375, 235.86328125)"))
* `r062_001_20-6` frame 9,036 (position / facing): 964 lost - Some((36, "pos: #2 sprite 280: (167.4453125, 270.2734375) vs game (167.1875, 268.875)"))
* `r062_001_20-6` frame 3,060 (camera): 940 lost - Some((60, "cam: (28.67578125, 183.9755859375) vs game (28.72265625, 182.9912109375)"))

**importer** (any-frame import) - tiles that differ at their first frame, by kind:

* camera `cam: (, ) vs game`: 1 tiles, 1,000 frames (earth)


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r002_006_4-1` | 002_d04_bandana | camera | 9,742 | Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |
| `r044_015_18-1` | 044_h_back1 | menu timing | 681 | Some((577, "fade: black true vs game false")) |
| `r012_014_12-19` | 012_db5_a01 | screen fade | 555 | Some((2567, "fade: black true vs game false")) |
| `r038_001_18-1` | 038_j03_goku120 | camera | 198 | Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)")) |

## Recurring first divergences (3+ segments)

* 3 x `cam`: `r002_006_4-1`, `r038_001_18-1`, `r067_071_14-2`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth


### ship


### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,338, **camera** - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload model on a 3D map
* `r012_014_12-19` (012_db5_a01): 2,567 / 3,122, **screen fade** - Some((2567, "fade: black true vs game false"))

### janemba

* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r044_015_18-1` (044_h_back1): 577 / 1,258, **menu timing** - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
