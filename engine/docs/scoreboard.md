# Parity scoreboard: the real playthrough on wf-core

Tree: `0dab418` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
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
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,646 | 8,440 | 371 | 368 | 380,206 | 366,480 | 96.4 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 253 | 187,291 | 186,412 | 99.5 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 446 | 467,701 | 441,978 | 94.5 |
| **all** |  | 1,668,050 | 45,294 | 1519 | 1512 | 1,622,756 | 1,582,428 | 97.5 |

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| menu timing | Earth agent (redraw / mixer timing) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| camera | chapter agent (camera scroll) | 0 | 0 | 0 | 2 | 1 | 1 | 4 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 0 | 0 | 1 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 2 segments, 25,723 frames lost

* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r067_071_14-2` (067_k03_goku158, camera): 1,722 lost - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 2 segments, 13,171 frames lost

* `r002_006_4-1` (002_d04_bandana, camera): 9,740 lost - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload model on a 3D map
* `r010_011_11-13` (010_db2_desert, camera): 3,431 lost - Some((1640, "cam: (240, 34.45703125) vs game (240, 34.71875)"))

**Earth agent (redraw / mixer timing)** - 1 segments, 681 frames lost

* `r044_015_18-1` (044_h_back1, menu timing): 681 lost - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

**cutscene / script-host agent** - 1 segments, 555 frames lost

* `r012_014_12-19` (012_db5_a01, screen fade): 555 lost - Some((2567, "fade: black true vs game false"))

**side-content agent** - 1 segments, 198 frames lost

* `r038_001_18-1` (038_j03_goku120, camera): 198 lost - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `0dab418`, tiles of 1,000 frames; the whole run 96.2% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**fat** (dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)) - 7 divergences inside tiles, 4,966 frames

* `r002_006_4-1` frame 6,008 (RNG): 992 lost - Some((8, "rng: [0d8cf402, 260df3cf] vs game [089b453c, a09338fc]"))
* `r002_006_4-1` frame 18,014 (animation): 986 lost - Some((14, "anim: #9 sprite 269: raw 5 cel 0 vs game raw 1 cel 0 tick 62"))
* `r010_011_11-13` frame 1,116 (animation): 884 lost - Some((16, "anim: #3 sprite 123: raw 1 cel 0 vs game raw 2 cel 0 tick 0"))
* `r002_006_4-1` frame 7,192 (sprite memory): 808 lost - Some((192, "vram: free / spare lists differ (same used blocks): [3] ours [1,371,1,2,16,255,255] game [1,371,1,2,15,255,2
* `r002_006_4-1` frame 19,292 (sprite memory): 708 lost - Some((292, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))

**superbuu** (Super Buu agent) - 21 divergences inside tiles, 18,175 frames

* `r062_001_20-6` frame 3,002 (sprite memory): 998 lost - Some((2, "vram: used blocks (start, size) ours only [], game only [(369, 8)]"))
* `r062_001_20-6` frame 20,011 (sprite memory): 989 lost - Some((11, "vram: used blocks (start, size) ours only [], game only [(408, 1)]"))
* `r062_001_20-6` frame 9,025 (RNG): 975 lost - Some((25, "rng: [a2751507, ee424df4] vs game [e69709d9, 799d8819]"))
* `r062_001_20-6` frame 7,026 (RNG): 974 lost - Some((26, "rng: [f5d56a63, 0f12b499] vs game [3fb5b6f3, 20803777]"))
* `r062_001_20-6` frame 16,031 (position / facing): 969 lost - Some((31, "pos: #2 sprite 280: (171.546875, 241.95703125) vs game (170.3671875, 240.88671875)"))

**importer** (any-frame import) - tiles that differ at their first frame, by kind:

* camera `cam: (, ) vs game`: 3 tiles, 2,310 frames (fat)


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r002_006_4-1` | 002_d04_bandana | camera | 9,740 | Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) |
| `r010_011_11-13` | 010_db2_desert | camera | 3,431 | Some((1640, "cam: (240, 34.45703125) vs game (240, 34.71875)")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |
| `r044_015_18-1` | 044_h_back1 | menu timing | 681 | Some((577, "fade: black true vs game false")) |
| `r012_014_12-19` | 012_db5_a01 | screen fade | 555 | Some((2567, "fade: black true vs game false")) |
| `r038_001_18-1` | 038_j03_goku120 | camera | 198 | Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)")) |

## Recurring first divergences (3+ segments)

* 4 x `cam`: `r002_006_4-1`, `r010_011_11-13`, `r038_001_18-1`, `r067_071_14-2`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth


### ship


### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336, **camera** - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload model on a 3D map
* `r010_011_11-13` (010_db2_desert): 1,618 / 5,049, **camera** - Some((1640, "cam: (240, 34.45703125) vs game (240, 34.71875)"))
* `r012_014_12-19` (012_db5_a01): 2,567 / 3,122, **screen fade** - Some((2567, "fade: black true vs game false"))

### janemba

* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r044_015_18-1` (044_h_back1): 577 / 1,258, **menu timing** - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
