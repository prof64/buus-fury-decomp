# Parity scoreboard: the real playthrough on wf-core

Tree: `90fa3f7` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
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
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,646 | 8,440 | 371 | 369 | 380,206 | 369,911 | 97.3 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 253 | 187,291 | 186,412 | 99.5 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 445 | 467,701 | 433,451 | 92.7 |
| **all** |  | 1,668,050 | 45,294 | 1519 | 1512 | 1,622,756 | 1,577,332 | 97.2 |

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| menu timing | Earth agent (redraw / mixer timing) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| camera | chapter agent (camera scroll) | 0 | 0 | 0 | 1 | 1 | 2 | 4 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 0 | 0 | 1 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 3 segments, 34,250 frames lost

* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r052_004_1-5` (052_m_gotenks, camera): 8,527 lost - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r067_071_14-2` (067_k03_goku158, camera): 1,722 lost - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 1 segments, 9,740 frames lost

* `r002_006_4-1` (002_d04_bandana, camera): 9,740 lost - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload model on a 3D map

**Earth agent (redraw / mixer timing)** - 1 segments, 681 frames lost

* `r044_015_18-1` (044_h_back1, menu timing): 681 lost - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

**cutscene / script-host agent** - 1 segments, 555 frames lost

* `r012_014_12-19` (012_db5_a01, screen fade): 555 lost - Some((2567, "fade: black true vs game false"))

**side-content agent** - 1 segments, 198 frames lost

* `r038_001_18-1` (038_j03_goku120, camera): 198 lost - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `90fa3f7`, tiles of 1,000 frames; the whole run 96.0% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**fat** (dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)) - 6 divergences inside tiles, 4,082 frames

* `r002_006_4-1` frame 6,008 (RNG): 992 lost - Some((8, "rng: [d7559509, b8a479f4] vs game [089b453c, a09338fc]"))
* `r002_006_4-1` frame 18,014 (animation): 986 lost - Some((14, "anim: #9 sprite 269: raw 5 cel 0 vs game raw 1 cel 0 tick 62"))
* `r002_006_4-1` frame 7,192 (sprite memory): 808 lost - Some((192, "vram: used blocks (start, size) ours only [(151, 4), (354, 16), (370, 1)], game only [(154, 1), (351, 16), (
* `r002_006_4-1` frame 19,292 (sprite memory): 708 lost - Some((292, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
* `r002_006_4-1` frame 10,596 (camera): 404 lost - Some((623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)"))

**superbuu** (Super Buu agent) - 20 divergences inside tiles, 17,338 frames

* `r062_001_20-6` frame 9,002 (sprite memory): 998 lost - Some((2, "vram: used blocks (start, size) ours only [(257, 64)], game only []"))
* `r062_001_20-6` frame 17,004 (RNG): 996 lost - Some((4, "rng: [aef64cec, 580be19f] vs game [e9c9e8c2, 2f580dcb]"))
* `r062_001_20-6` frame 14,005 (sound, Earth agent (sound timing) / audio): 995 lost - Some((5, "snd: [] vs game [\"play 34 64\", \"stop 3\"]"))
* `r062_001_20-6` frame 11,006 (sprite memory): 994 lost - Some((6, "vram: used blocks (start, size) ours only [], game only [(377, 8)]"))
* `r062_001_20-6` frame 20,011 (sprite memory): 989 lost - Some((11, "vram: used blocks (start, size) ours only [], game only [(408, 1)]"))

**importer** (any-frame import) - tiles that differ at their first frame, by kind:

* position / facing `pos: sprite : (, ) vs game`: 2 tiles, 2,000 frames (superbuu)
* camera `cam: (, ) vs game`: 2 tiles, 1,310 frames (fat, superbuu)
* screen fade `fade: black true vs game false`: 1 tiles, 1,000 frames (superbuu)
* animation `anim: sprite : raw  cel  vs game raw  ce`: 1 tiles, 1,000 frames (superbuu)


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r002_006_4-1` | 002_d04_bandana | camera | 9,740 | Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) |
| `r052_004_1-5` | 052_m_gotenks | camera | 8,527 | Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |
| `r044_015_18-1` | 044_h_back1 | menu timing | 681 | Some((577, "fade: black true vs game false")) |
| `r012_014_12-19` | 012_db5_a01 | screen fade | 555 | Some((2567, "fade: black true vs game false")) |
| `r038_001_18-1` | 038_j03_goku120 | camera | 198 | Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)")) |

## Recurring first divergences (3+ segments)

* 4 x `cam`: `r002_006_4-1`, `r038_001_18-1`, `r052_004_1-5`, `r067_071_14-2`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth


### ship


### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336, **camera** - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload model on a 3D map
* `r012_014_12-19` (012_db5_a01): 2,567 / 3,122, **screen fade** - Some((2567, "fade: black true vs game false"))

### janemba

* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r044_015_18-1` (044_h_back1): 577 / 1,258, **menu timing** - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

### superbuu

* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084, **camera** - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
