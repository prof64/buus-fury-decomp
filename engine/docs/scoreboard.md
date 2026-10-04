# Parity scoreboard: the real playthrough on wf-core

Tree: `aba7278` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
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
| ship | ship agent | 398,194 | 2,578 | 338 | 331 | 395,616 | 344,290 | 87.0 |
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,648 | 8,439 | 371 | 364 | 380,209 | 365,618 | 96.2 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 253 | 187,291 | 184,981 | 98.8 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 443 | 467,701 | 379,133 | 81.1 |
| **all** |  | 1,668,052 | 45,293 | 1519 | 1498 | 1,622,759 | 1,465,964 | 90.3 |

**Whole run, no imports, no lag frames (the target: one step per game logic iteration): exact to frame 90,251** of 1,668,052 recorded (E2_earth; first divergence: pos: app (24576, 8192, 0) game (22766, 10002, 0)). Tree `78590a3`, `tools/wholerun/board.py`.

**Whole run, no imports, recorded lag injected (alignment check): exact to frame 137,895** of 1,668,052 recorded (E2_earth; first divergence: rng: app [7357c793, a04234d5] game [87ffe884, f99c6366]). Tree `78590a3`, `tools/wholerun/board.py`.

## Whole run with resync: every stop, per owner

The app from the reset; at each stop the game's state is cut at the next row and the run goes on (`tools/wholerun/resync.py`). "rule" = the app diverged from a state it had right; "import" = the cut was refused or diverged in its first rows (importer). Each group: count, first stop.

**rec lag:** 19 stops (10 rule, 9 import)

| count | kind | field | chapter | owner | first row | first stop |
|---|---|---|---|---|---|---|
| 2 | rule | rng | earth | Earth agent | 153,225 | app [ddf7897e, 38c3c1f0] game [d800841f, 1450638f] |
| 2 | rule | area | earth | Earth agent | 153,389 | app (7, 5) game (7, 3) |
| 2 | rule | pos | fat | dungeon agents A / B | 621,338 | app (73530, 81716, 3) game (73722, 81716, 3) |
| 1 | rule | pos | ship | ship agent | 519,616 | app (23980, 22486, 0) game (23980, 22102, 0) |
| 1 | rule | rng | ship | ship agent | 519,665 | app [24dbf79b, 815a72e5] game [a6fdc6e9, b7fba9d2] |
| 1 | rule | rng | fat | dungeon agents A / B | 608,823 | app [2b6668bc, 0ae5d8e3] game [77786984, bc24e457] |
| 1 | rule | depth | fat | dungeon agents A / B | 642,083 | app 1 game 0 |
| 6 | import | area | fat | importer agent | 642,084 | app (8, 3) game (1, 1) |
| 2 | import | import | ? | importer agent | 643,974 | unsupported cut: area AreaId { world: 8, area: 6, variant: 1 }: the load's on-enter script |
| 1 | import | rng | fat | importer agent | 621,339 | app [2302d396, 28b8d03c] game [40e67a74, 60b0e573] |

**sim lag:** 120 stops (76 rule, 44 import)

| count | kind | field | chapter | owner | first row | first stop |
|---|---|---|---|---|---|---|
| 18 | rule | rng | ship | ship agent | 208,396 | app [17d43d9c, abb04a12] game [b12fb01f, d29a98bb] |
| 15 | rule | pos | ship | ship agent | 208,365 | app (57344, 197632, 1) game (57344, 197504, 1) |
| 13 | rule | pos | earth | Earth agent | 48,858 | app (24832, 79872, 1) game (24832, 79744, 1) |
| 7 | rule | rng | earth | Earth agent | 58,432 | app [6724459b, 4a4e8f46] game [23be8d4a, 7b19df0b] |
| 7 | rule | pos | fat | dungeon agents A / B | 610,699 | app (68608, 75776, 0) game (68608, 75904, 0) |
| 5 | rule | area | earth | Earth agent | 59,815 | app (4, 20) game (4, 27) |
| 5 | rule | rng | fat | dungeon agents A / B | 609,360 | app [d762541a, 35a3204f] game [04569d93, 290569ac] |
| 2 | rule | rng | opening | Earth agent | 21,437 | app [12607523, fe6402b0] game [eb483324, 121adeb2] |
| 1 | rule | area | opening | Earth agent | 4,691 | app (0, 96) game (1, 1) |
| 1 | rule | pos | opening | Earth agent | 21,406 | app (75776, 66560, 1) game (75776, 66432, 1) |
| 1 | rule | area | ship | ship agent | 564,445 | app (8, 71) game (8, 70) |
| 1 | rule | area | fat | dungeon agents A / B | 639,192 | app (8, 3) game (1, 1) |
| 13 | import | pos | earth | importer agent | 48,862 | app (24832, 79232, 0) game (24832, 79232, 1) |
| 7 | import | area | fat | importer agent | 639,193 | app (8, 3) game (1, 1) |
| 5 | import | import | ? | importer agent | 631,386 | unsupported cut: area AreaId { world: 8, area: 3, variant: 1 }: the load's on-enter script |
| 4 | import | pos | ship | importer agent | 225,250 | app (49152, 13312, 0) game (49152, 13565, 0) |
| 4 | import | rng | ship | importer agent | 547,558 | app [4f056e4f, 515cd154] game [5e7e7ff3, a87fca59] |
| 4 | import | pos | fat | importer agent | 631,416 | app (110592, 130816, 1) game (110592, 130560, 1) |
| 3 | import | rng | opening | importer agent | 41,536 | app [f2baedee, 3f3f0bf7] game [a784cfbe, 58ea61c1] |
| 3 | import | rng | earth | importer agent | 72,280 | app [ad37d6f5, ce7018ac] game [e3c98bcb, 4ba29ec9] |
| 1 | import | rng | fat | importer agent | 621,339 | app [2302d396, 28b8d03c] game [40e67a74, 60b0e573] |


## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 2 | 0 | 1 | 3 | 6 |
| object set | chapter agent (spawns / removal) | 0 | 0 | 5 | 7 | 0 | 2 | 14 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 5 segments, 88,568 frames lost

* `r069_168_14-1` (069_k05_vegeta150, object set): 55,503 lost - Some((330, "missing: obj #12 sprite 115 has no entity")) **Known:** Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore's black lasts 26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the 26th frame)
* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r064_029_21-26` (064_m_kid_buu_saga, position / facing): 7,862 lost - Some((12640, "pos: #212 sprite 115: (149.375, 583.13671875) vs game (132061.875, 131797.9375)"))
* `r067_136_14-1` (067_k03_goku158, position / facing): 794 lost - Some((319, "pos: #12 sprite 115: (98.625, 241.125) vs game (-5657944.69140625, -5461077)")) **Known:** Switch Character at 14-1 (Goku -> Vegeta): game 25 black frames, predicted 24 (46,986 short); the same save point's Trunks <-> Goten switches (r017_037) predict right with margins 86 k / 50 k
* `r064_017_21-14` (064_m_kid_buu_saga, object set): 408 lost - Some((507, "missing: obj #17 sprite 115 has no entity")) **Known:** Switch Character at 21-14 (Vegeta -> Goku, 4 figures): predicted 15, the game 14 (over by >= 99 k cycles: CHARSEL_CLOSE_SPRITE x 4?)

**ship agent** - 7 segments, 51,326 frames lost

* `r000_328_8-66` (000_ship_c14, object set): 47,098 lost - Some((264, "missing: obj #14 sprite 117 has no entity"))
* `r000_002_7-2` (000_ship_c14, object set): 1,494 lost - Some((18367, "missing: obj #115 sprite 117 has no entity"))
* `r000_226_8-1` (000_ship_c14, object set): 1,486 lost - Some((255, "missing: obj #8 sprite 117 has no entity"))
* `r000_172_8-1` (000_ship_c14, object set): 354 lost - Some((241, "missing: obj #8 sprite 117 has no entity"))
* `r000_056_8-1` (000_ship_c14, position / facing): 342 lost - Some((245, "pos: #8 sprite 117: (148.625, 182.875) vs game (132062.953125, 132062.6875)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 7 segments, 14,591 frames lost

* `r009_006_4-1` (009_d16_desert, object set): 9,174 lost - Some((313, "missing: obj #18 sprite 117 has no entity"))
* `r003_090_8-1` (003_d06_thieves, object set): 1,461 lost - Some((1740, "missing: obj #124 sprite 117 has no entity"))
* `r004_022_9-1` (004_d10_map, object set): 1,176 lost - Some((1497, "missing: obj #128 sprite 117 has no entity"))
* `r002_004_4-1` (002_d04_bandana, object set): 936 lost - Some((596, "missing: obj #17 sprite 117 has no entity"))
* `r017_037_13-1` (017_db6_fukurou, object set): 924 lost - Some((1869, "missing: obj #129 sprite 117 has no entity"))

**cutscene / script-host agent** - 1 segments, 1,453 frames lost

* `r047_002_18-13` (047_h_h13, screen fade): 1,453 lost - Some((1470, "fade: black false vs game true")) **Known:** heal menu / Switch Character in 18-13: one frame short

**side-content agent** - 1 segments, 857 frames lost

* `r044_015_18-1` (044_h_back1, position / facing): 857 lost - Some((403, "pos: #9 sprite 117: (104.75, 258.5) vs game (131858.546875, 132062.109375)")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `78590a3`, tiles of 1,000 frames; the whole run 96.8% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**earth** (Earth agent) - 2 divergences inside tiles, 1,881 frames

* `rE2__087_2-90` frame 5,051 (sprite memory): 949 lost - Some((51, "vram: used blocks (start, size) ours only [(447, 16)], game only []"))
* `rE2__087_2-90` frame 1,068 (sprite memory): 932 lost - Some((68, "vram: used blocks (start, size) ours only [(439, 16)], game only []"))

**superbuu** (Super Buu agent) - 16 divergences inside tiles, 12,735 frames

* `r062_001_20-6` frame 7,027 (position / facing): 973 lost - Some((27, "pos: #2 sprite 280: (234.90625, 266.58203125) vs game (234.20703125, 267.14453125)"))
* `r062_001_20-6` frame 16,031 (position / facing): 969 lost - Some((31, "pos: #2 sprite 280: (171.546875, 241.95703125) vs game (170.3671875, 240.88671875)"))
* `r062_001_20-6` frame 12,035 (position / facing): 965 lost - Some((35, "pos: #2 sprite 280: (164.8828125, 236.5625) vs game (164.359375, 235.86328125)"))
* `r062_001_20-6` frame 9,036 (position / facing): 964 lost - Some((36, "pos: #2 sprite 280: (167.4453125, 270.2734375) vs game (167.1875, 268.875)"))
* `r062_001_20-6` frame 10,085 (position / facing): 915 lost - Some((85, "pos: #2 sprite 280: (181.08984375, 191.49609375) vs game (180.390625, 191.34765625)"))

**importer** (any-frame import) - tiles that differ at their first frame, by kind:

* camera `cam: (, ) vs game`: 1 tiles, 1,000 frames (earth)


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r069_168_14-1` | 069_k05_vegeta150 | object set | 55,503 | Some((330, "missing: obj #12 sprite 115 has no entity")) |
| `r000_328_8-66` | 000_ship_c14 | object set | 47,098 | Some((264, "missing: obj #14 sprite 117 has no entity")) |
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r009_006_4-1` | 009_d16_desert | object set | 9,174 | Some((313, "missing: obj #18 sprite 117 has no entity")) |
| `r064_029_21-26` | 064_m_kid_buu_saga | position / facing | 7,862 | Some((12640, "pos: #212 sprite 115: (149.375, 583.13671875) vs game (132061.875, 131797.9375)")) |
| `r000_002_7-2` | 000_ship_c14 | object set | 1,494 | Some((18367, "missing: obj #115 sprite 117 has no entity")) |
| `r000_226_8-1` | 000_ship_c14 | object set | 1,486 | Some((255, "missing: obj #8 sprite 117 has no entity")) |
| `r003_090_8-1` | 003_d06_thieves | object set | 1,461 | Some((1740, "missing: obj #124 sprite 117 has no entity")) |
| `r047_002_18-13` | 047_h_h13 | screen fade | 1,453 | Some((1470, "fade: black false vs game true")) |
| `r004_022_9-1` | 004_d10_map | object set | 1,176 | Some((1497, "missing: obj #128 sprite 117 has no entity")) |
| `r002_004_4-1` | 002_d04_bandana | object set | 936 | Some((596, "missing: obj #17 sprite 117 has no entity")) |
| `r017_037_13-1` | 017_db6_fukurou | object set | 924 | Some((1869, "missing: obj #129 sprite 117 has no entity")) |
| `r044_015_18-1` | 044_h_back1 | position / facing | 857 | Some((403, "pos: #9 sprite 117: (104.75, 258.5) vs game (131858.546875, 132062.109375)")) |
| `r067_136_14-1` | 067_k03_goku158 | position / facing | 794 | Some((319, "pos: #12 sprite 115: (98.625, 241.125) vs game (-5657944.69140625, -5461077)")) |
| `r008_003_10-30` | 008_d15_map | object set | 583 | Some((272, "missing: obj #19 sprite 117 has no entity")) |

## Recurring first divergences (3+ segments)

* 14 x `missing`: `r000_002_7-2`, `r000_172_8-1`, `r000_226_8-1`, `r000_250_8-26`, `r000_328_8-66`, `r002_004_4-1`, `r003_018_8-1`, `r003_090_8-1` ...
* 3 x `pos sprite 117`: `r000_056_8-1`, `r000_266_8-38`, `r044_015_18-1`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth


### ship

* `r000_002_7-2` (000_ship_c14): 18,365 / 19,859, **object set** - Some((18367, "missing: obj #115 sprite 117 has no entity"))
* `r000_056_8-1` (000_ship_c14): 243 / 585, **position / facing** - Some((245, "pos: #8 sprite 117: (148.625, 182.875) vs game (132062.953125, 132062.6875)"))
* `r000_172_8-1` (000_ship_c14): 239 / 593, **object set** - Some((241, "missing: obj #8 sprite 117 has no entity"))
* `r000_226_8-1` (000_ship_c14): 253 / 1,739, **object set** - Some((255, "missing: obj #8 sprite 117 has no entity"))
* `r000_250_8-26` (000_ship_c14): 4,862 / 5,153, **object set** - Some((4864, "missing: obj #35 sprite 117 has no entity"))
* `r000_266_8-38` (000_ship_c14): 11,585 / 11,846, **position / facing** - Some((11587, "pos: #69 sprite 117: (61.125, 310.9375) vs game (0.78125, 1)"))
* `r000_328_8-66` (000_ship_c14): 262 / 47,360, **object set** - Some((264, "missing: obj #14 sprite 117 has no entity"))

### fat

* `r002_004_4-1` (002_d04_bandana): 594 / 1,530, **object set** - Some((596, "missing: obj #17 sprite 117 has no entity"))
* `r003_018_8-1` (003_d06_thieves): 234 / 571, **object set** - Some((236, "missing: obj #8 sprite 117 has no entity"))
* `r003_090_8-1` (003_d06_thieves): 1,738 / 3,199, **object set** - Some((1740, "missing: obj #124 sprite 117 has no entity"))
* `r004_022_9-1` (004_d10_map): 1,495 / 2,671, **object set** - Some((1497, "missing: obj #128 sprite 117 has no entity"))
* `r008_003_10-30` (008_d15_map): 270 / 853, **object set** - Some((272, "missing: obj #19 sprite 117 has no entity"))
* `r009_006_4-1` (009_d16_desert): 311 / 9,485, **object set** - Some((313, "missing: obj #18 sprite 117 has no entity"))
* `r017_037_13-1` (017_db6_fukurou): 1,867 / 2,791, **object set** - Some((1869, "missing: obj #129 sprite 117 has no entity"))

### janemba

* `r044_015_18-1` (044_h_back1): 401 / 1,258, **position / facing** - Some((403, "pos: #9 sprite 117: (104.75, 258.5) vs game (131858.546875, 132062.109375)")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)
* `r047_002_18-13` (047_h_h13): 1,349 / 2,802, **screen fade** - Some((1470, "fade: black false vs game true")) **Known:** heal menu / Switch Character in 18-13: one frame short

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r064_017_21-14` (064_m_kid_buu_saga): 505 / 913, **object set** - Some((507, "missing: obj #17 sprite 115 has no entity")) **Known:** Switch Character at 21-14 (Vegeta -> Goku, 4 figures): predicted 15, the game 14 (over by >= 99 k cycles: CHARSEL_CLOSE_SPRITE x 4?)
* `r064_029_21-26` (064_m_kid_buu_saga): 12,638 / 20,500, **position / facing** - Some((12640, "pos: #212 sprite 115: (149.375, 583.13671875) vs game (132061.875, 131797.9375)"))
* `r067_136_14-1` (067_k03_goku158): 317 / 1,111, **position / facing** - Some((319, "pos: #12 sprite 115: (98.625, 241.125) vs game (-5657944.69140625, -5461077)")) **Known:** Switch Character at 14-1 (Goku -> Vegeta): game 25 black frames, predicted 24 (46,986 short); the same save point's Trunks <-> Goten switches (r017_037) predict right with margins 86 k / 50 k
* `r069_168_14-1` (069_k05_vegeta150): 328 / 55,831, **object set** - Some((330, "missing: obj #12 sprite 115 has no entity")) **Known:** Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore's black lasts 26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the 26th frame)
