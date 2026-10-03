# Parity scoreboard: the real playthrough on wf-core

Tree: `604927f` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
(the cuts: `scoreboard.py cut`, local under `work/real/`; the run itself: `tools/segments/realrun.py`, §"How to replay
the real run" in `engine/docs/segments.md`). A segment = the real game from one importable area entry (an exit
arrival with nothing running) to the next; an entry by a script warp / in a cutscene is not a cut - its frames
stay in the segment before it, and a segment runs on across the boundaries of the run's key logs (`scoreboard.py
extend`). "not cut" = frames in no segment (the run's start up to its first importable entry) - a chapter's
frames are counted where its segments start.

| chapter | owner | run frames | not cut | segments | exact | frames | matched | % |
|---|---|---|---|---|---|---|---|---|
| opening | Earth agent (title, New Game, Other World) | 42,275 | 7,898 | 17 | 17 | 34,377 | 34,377 | 100.0 |
| earth | Earth agent | 160,503 | 2,938 | 90 | 87 | 157,565 | 107,546 | 68.3 |
| ship | ship agent | 398,194 | 28,129 | 338 | 336 | 370,065 | 360,140 | 97.3 |
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,646 | -93,228 | 371 | 363 | 481,874 | 340,804 | 70.7 |
| janemba | side-content agent | 199,703 | 31,327 | 255 | 248 | 168,376 | 156,630 | 93.0 |
| superbuu | Super Buu agent | 478,729 | 114,022 | 448 | 404 | 364,707 | 299,655 | 82.2 |
| **all** |  | 1,668,050 | 91,086 | 1519 | 1455 | 1,576,964 | 1,299,152 | 82.4 |

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| sound | Earth agent (sound timing) / audio | 0 | 0 | 1 | 2 | 0 | 1 | 4 |
| RNG | chapter agent (who draws first: enemy AI / hits) | 0 | 1 | 0 | 1 | 2 | 15 | 19 |
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 0 | 1 | 1 | 2 |
| camera | chapter agent (camera scroll) | 0 | 0 | 1 | 1 | 2 | 4 | 8 |
| animation | chapter agent | 0 | 2 | 0 | 1 | 0 | 0 | 3 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 3 | 1 | 22 | 26 |
| object set | chapter agent (spawns / removal) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| other | - | 0 | 0 | 0 | 0 | 0 | 1 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Earth agent (sound timing) / audio** - 4 segments, 124,846 frames lost

* `r002_006_4-1` (002_d04_bandana, sound): 104,745 lost - Some((18132, "snd: [] vs game [\"play 40 64\"]"))
* `r061_004_20-5` (061_g05_goku141, sound): 11,191 lost - Some((5251, "snd: [] vs game [\"play 41 32\"]"))
* `r000_328_8-66` (000_ship_c14, sound): 8,516 lost - Some((38844, "snd: [] vs game [\"play 54 64\"]"))
* `r004_022_9-1` (004_d10_map, sound): 394 lost - Some((1271, "snd: [\"play 40 64\"] vs game []"))

**Earth agent** - 3 segments, 50,019 frames lost

* `rE2__090_7-3` (E2_earth, animation): 45,757 lost - Some((6658, "anim: #42 sprite 222: raw 1 cel 0 tick 1776 vs game raw 1 cel 1 tick 1777"))
* `rE2__087_2-90` (E2_earth, RNG): 3,424 lost - Some((6646, "rng: [cf71815a, 12e050f2] vs game [c0dd6d39, 5de832a6]"))
* `rE2__089_7-2` (E2_earth, animation): 838 lost - Some((1432, "anim: #17 sprite 233: raw 1 cel 3 tick 29 vs game raw 1 cel 0 tick 41"))

**Super Buu agent** - 41 segments, 49,959 frames lost

* `r062_001_20-6` (062_m_inside_buu, RNG): 12,951 lost - Some((478, "rng: [8d392598, 71aa1930] vs game [64e0af92, 9cba1858]"))
* `r052_004_1-5` (052_m_gotenks, camera): 8,527 lost - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r069_161_14-2` (069_k05_vegeta150, position / facing): 2,008 lost - Some((166, "pos: #3 sprite 256: (89.8203125, 130.578125) vs game (91.8203125, 130.578125)"))
* `r069_125_14-2` (069_k05_vegeta150, RNG): 1,925 lost - Some((87, "rng: [5f0fda5a, 018eaccf] vs game [60553a04, 72fd0483]"))
* `r067_071_14-2` (067_k03_goku158, camera): 1,722 lost - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 6 segments, 35,931 frames lost

* `r007_013_10-31` (007_d13_pilaf_won, position / facing): 17,190 lost - Some((3400, "pos: #6 sprite 164: (167.8515625, 151.95703125) vs game (167.8515625, 152)"))
* `r028_019_15-1` (028_db4b_end, animation): 15,809 lost - Some((495, "anim: #16 sprite 123: raw 4 cel 0 vs game raw 1 cel 0 tick 185"))
* `r012_014_12-19` (012_db5_a01, position / facing): 1,629 lost - Some((1493, "pos: #27 sprite 188: (294.08984375, 121.76171875) vs game (294.38671875, 122.05859375)"))
* `r002_004_4-1` (002_d04_bandana, camera): 851 lost - Some((741, "cam: (37.5556640625, 512.0166015625) vs game (37.642578125, 512.0224609375)"))
* `r017_009_13-45` (017_db6_fukurou, RNG): 249 lost - Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]"))

**side-content agent** - 6 segments, 11,571 frames lost

* `r049_001_18-14` (049_j_sj2, position / facing): 8,299 lost - Some((1069, "pos: #4 sprite 147: (116, 68) vs game (116, 67)"))
* `r047_002_18-13` (047_h_h13, camera): 1,357 lost - Some((2143, "cam: (0, 102.0234375) vs game (0, 102.025390625)")) **Known:** heal menu / Switch Character in 18-13: one frame short
* `r046_045_18-2` (046_h_veg1, RNG): 928 lost - Some((754, "rng: [4f3cec76, d4632e53] vs game [866b86b4, 2f27cc19]"))
* `r038_045_18-1` (038_j03_goku120, object set): 567 lost - Some((251, "missing: obj #1 sprite 151 has no entity"))
* `r037_084_18-1` (037_j02_goku90, RNG): 222 lost - Some((465, "rng: [06df649d, 58f5c11f] vs game [515ee011, 9cbe0118]"))

**-** - 1 segments, 3,600 frames lost

* `r064_029_21-26` (064_m_kid_buu_saga, other): 3,600 lost - Some((14604, "area: (0, 0, 0) vs game (0, 2, 1)"))

**ship agent** - 1 segments, 1,409 frames lost

* `r000_002_7-2` (000_ship_c14, camera): 1,409 lost - Some((18475, "cam: (37.44140625, 102.95703125) vs game (37.419921875, 102.9599609375)"))

**cutscene / script-host agent** - 2 segments, 477 frames lost

* `r064_017_21-14` (064_m_kid_buu_saga, screen fade): 302 lost - Some((611, "fade: black true vs game false")) **Known:** Switch Character at 21-14 (Vegeta -> Goku, 4 figures): predicted 15, the game 14 (over by >= 99 k cycles: CHARSEL_CLOSE_SPRITE x 4?)
* `r038_002_18-2` (038_j03_goku120, screen fade): 175 lost - Some((1563, "fade: black false vs game true")) **Known:** a heal menu closed in 18-2: predicted 21 black frames 208 cycles before the 22nd


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r002_006_4-1` | 002_d04_bandana | sound | 104,745 | Some((18132, "snd: [] vs game [\"play 40 64\"]")) |
| `rE2__090_7-3` | E2_earth | animation | 45,757 | Some((6658, "anim: #42 sprite 222: raw 1 cel 0 tick 1776 vs game raw 1 cel 1 tick 1777")) |
| `r007_013_10-31` | 007_d13_pilaf_won | position / facing | 17,190 | Some((3400, "pos: #6 sprite 164: (167.8515625, 151.95703125) vs game (167.8515625, 152)")) |
| `r028_019_15-1` | 028_db4b_end | animation | 15,809 | Some((495, "anim: #16 sprite 123: raw 4 cel 0 vs game raw 1 cel 0 tick 185")) |
| `r062_001_20-6` | 062_m_inside_buu | RNG | 12,951 | Some((478, "rng: [8d392598, 71aa1930] vs game [64e0af92, 9cba1858]")) |
| `r061_004_20-5` | 061_g05_goku141 | sound | 11,191 | Some((5251, "snd: [] vs game [\"play 41 32\"]")) |
| `r052_004_1-5` | 052_m_gotenks | camera | 8,527 | Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)")) |
| `r000_328_8-66` | 000_ship_c14 | sound | 8,516 | Some((38844, "snd: [] vs game [\"play 54 64\"]")) |
| `r049_001_18-14` | 049_j_sj2 | position / facing | 8,299 | Some((1069, "pos: #4 sprite 147: (116, 68) vs game (116, 67)")) |
| `r064_029_21-26` | 064_m_kid_buu_saga | other | 3,600 | Some((14604, "area: (0, 0, 0) vs game (0, 2, 1)")) |
| `rE2__087_2-90` | E2_earth | RNG | 3,424 | Some((6646, "rng: [cf71815a, 12e050f2] vs game [c0dd6d39, 5de832a6]")) |
| `r069_161_14-2` | 069_k05_vegeta150 | position / facing | 2,008 | Some((166, "pos: #3 sprite 256: (89.8203125, 130.578125) vs game (91.8203125, 130.578125)")) |
| `r069_125_14-2` | 069_k05_vegeta150 | RNG | 1,925 | Some((87, "rng: [5f0fda5a, 018eaccf] vs game [60553a04, 72fd0483]")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |
| `r012_014_12-19` | 012_db5_a01 | position / facing | 1,629 | Some((1493, "pos: #27 sprite 188: (294.08984375, 121.76171875) vs game (294.38671875, 122.05859375)")) |

## Recurring first divergences (3+ segments)

* 19 x `pos sprite 256`: `r057_005_14-2`, `r057_013_14-2`, `r057_015_14-2`, `r057_021_14-2`, `r057_059_14-2`, `r069_003_14-2`, `r069_013_14-2`, `r069_021_14-2` ...
* 19 x `rng`: `r017_009_13-45`, `r037_084_18-1`, `r046_045_18-2`, `r057_011_14-2`, `r057_031_14-2`, `r057_035_14-2`, `r062_001_20-6`, `r069_011_14-2` ...
* 8 x `cam`: `r000_002_7-2`, `r002_004_4-1`, `r038_001_18-1`, `r047_002_18-13`, `r052_004_1-5`, `r061_017_20-6`, `r061_019_20-6`, `r067_071_14-2`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth

* `rE2__087_2-90` (E2_earth): 6,646 / 10,070, **RNG** - Some((6646, "rng: [cf71815a, 12e050f2] vs game [c0dd6d39, 5de832a6]"))
* `rE2__089_7-2` (E2_earth): 1,432 / 2,270, **animation** - Some((1432, "anim: #17 sprite 233: raw 1 cel 3 tick 29 vs game raw 1 cel 0 tick 41"))
* `rE2__090_7-3` (E2_earth): 6,658 / 52,415, **animation** - Some((6658, "anim: #42 sprite 222: raw 1 cel 0 tick 1776 vs game raw 1 cel 1 tick 1777"))

### ship

* `r000_002_7-2` (000_ship_c14): 18,450 / 19,859, **camera** - Some((18475, "cam: (37.44140625, 102.95703125) vs game (37.419921875, 102.9599609375)"))
* `r000_328_8-66` (000_ship_c14): 38,844 / 47,360, **sound** - Some((38844, "snd: [] vs game [\"play 54 64\"]"))

### fat

* `r002_004_4-1` (002_d04_bandana): 679 / 1,530, **camera** - Some((741, "cam: (37.5556640625, 512.0166015625) vs game (37.642578125, 512.0224609375)"))
* `r002_006_4-1` (002_d04_bandana): 18,132 / 122,877, **sound** - Some((18132, "snd: [] vs game [\"play 40 64\"]"))
* `r004_022_9-1` (004_d10_map): 1,269 / 1,663, **sound** - Some((1271, "snd: [\"play 40 64\"] vs game []"))
* `r007_013_10-31` (007_d13_pilaf_won): 3,400 / 20,590, **position / facing** - Some((3400, "pos: #6 sprite 164: (167.8515625, 151.95703125) vs game (167.8515625, 152)"))
* `r012_014_12-19` (012_db5_a01): 1,493 / 3,122, **position / facing** - Some((1493, "pos: #27 sprite 188: (294.08984375, 121.76171875) vs game (294.38671875, 122.05859375)"))
* `r017_009_13-45` (017_db6_fukurou): 270 / 519, **RNG** - Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]"))
* `r017_022_13-47` (017_db6_fukurou): 225 / 428, **position / facing** - Some((225, "pos: #4 sprite 238: (351.90625, 207.46875) vs game (347.90625, 203.46875)"))
* `r028_019_15-1` (028_db4b_end): 495 / 16,304, **animation** - Some((495, "anim: #16 sprite 123: raw 4 cel 0 vs game raw 1 cel 0 tick 185"))

### janemba

* `r037_084_18-1` (037_j02_goku90): 465 / 687, **RNG** - Some((465, "rng: [06df649d, 58f5c11f] vs game [515ee011, 9cbe0118]"))
* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r038_002_18-2` (038_j03_goku120): 1,542 / 1,717, **screen fade** - Some((1563, "fade: black false vs game true")) **Known:** a heal menu closed in 18-2: predicted 21 black frames 208 cycles before the 22nd
* `r038_045_18-1` (038_j03_goku120): 251 / 818, **object set** - Some((251, "missing: obj #1 sprite 151 has no entity"))
* `r046_045_18-2` (046_h_veg1): 754 / 1,682, **RNG** - Some((754, "rng: [4f3cec76, d4632e53] vs game [866b86b4, 2f27cc19]"))
* `r047_002_18-13` (047_h_h13): 2,129 / 3,486, **camera** - Some((2143, "cam: (0, 102.0234375) vs game (0, 102.025390625)")) **Known:** heal menu / Switch Character in 18-13: one frame short
* `r049_001_18-14` (049_j_sj2): 1,069 / 9,368, **position / facing** - Some((1069, "pos: #4 sprite 147: (116, 68) vs game (116, 67)"))

### superbuu

* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084, **camera** - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r057_005_14-2` (057_g03_gohan141): 748 / 1,123, **position / facing** - Some((748, "pos: #4 sprite 256: (200, 200) vs game (200, 198)"))
* `r057_011_14-2` (057_g03_gohan141): 304 / 678, **RNG** - Some((304, "rng: [32c37f60, 7161f349] vs game [44eec898, d5acbac2]"))
* `r057_013_14-2` (057_g03_gohan141): 677 / 1,575, **position / facing** - Some((677, "pos: #4 sprite 256: (203.1875, 201.15234375) vs game (203.984375, 201.38671875)"))
* `r057_015_14-2` (057_g03_gohan141): 348 / 677, **position / facing** - Some((348, "pos: #3 sprite 256: (109.921875, 125) vs game (111.921875, 125)"))
* `r057_021_14-2` (057_g03_gohan141): 537 / 859, **position / facing** - Some((537, "pos: #4 sprite 256: (262.15625, 218.8515625) vs game (262.15625, 220.8515625)"))
* `r057_031_14-2` (057_g03_gohan141): 564 / 864, **RNG** - Some((564, "rng: [7d11f712, 74a63311] vs game [0b7b8092, a01ff008]"))
* `r057_035_14-2` (057_g03_gohan141): 626 / 914, **RNG** - Some((626, "rng: [e2c5877d, 6e8ce462] vs game [8b1da78c, f1845cae]"))
* `r057_059_14-2` (057_g03_gohan141): 650 / 1,800, **position / facing** - Some((650, "pos: #4 sprite 256: (180.078125, 200.984375) vs game (179.28125, 200.984375)"))
* `r061_004_20-5` (061_g05_goku141): 5,251 / 16,442, **sound** - Some((5251, "snd: [] vs game [\"play 41 32\"]"))
* `r061_017_20-6` (061_g05_goku141): 897 / 1,183, **camera** - Some((911, "cam: (32, 80.9140625) vs game (32, 80)"))
* `r061_019_20-6` (061_g05_goku141): 841 / 1,135, **camera** - Some((855, "cam: (21, 80.796875) vs game (21, 80.7060546875)"))
* `r062_001_20-6` (062_m_inside_buu): 476 / 13,427, **RNG** - Some((478, "rng: [8d392598, 71aa1930] vs game [64e0af92, 9cba1858]"))
* `r064_004_21-6` (064_m_kid_buu_saga): 240 / 1,041, **position / facing** - Some((240, "pos: #9 sprite 87: (80, 176) vs game (168, 280)"))
* `r064_017_21-14` (064_m_kid_buu_saga): 611 / 913, **screen fade** - Some((611, "fade: black true vs game false")) **Known:** Switch Character at 21-14 (Vegeta -> Goku, 4 figures): predicted 15, the game 14 (over by >= 99 k cycles: CHARSEL_CLOSE_SPRITE x 4?)
* `r064_029_21-26` (064_m_kid_buu_saga): 14,604 / 18,204, **other** - Some((14604, "area: (0, 0, 0) vs game (0, 2, 1)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
* `r069_003_14-2` (069_k05_vegeta150): 413 / 1,554, **position / facing** - Some((413, "pos: #3 sprite 256: (109.453125, 135.74609375) vs game (109.453125, 137.74609375)"))
* `r069_011_14-2` (069_k05_vegeta150): 475 / 1,322, **RNG** - Some((475, "rng: [f576fbd5, 16637fb0] vs game [ee21c7f6, 576f1279]"))
* `r069_013_14-2` (069_k05_vegeta150): 772 / 1,044, **position / facing** - Some((772, "pos: #3 sprite 256: (66.9375, 202.13671875) vs game (66.140625, 202.13671875)"))
* `r069_021_14-2` (069_k05_vegeta150): 576 / 2,149, **position / facing** - Some((576, "pos: #3 sprite 256: (103.546875, 116.1015625) vs game (103.546875, 118.1015625)"))
* `r069_027_14-2` (069_k05_vegeta150): 939 / 1,489, **RNG** - Some((939, "rng: [efdd8768, 2b02fbc6] vs game [d9abb81f, 87e10e5f]"))
* `r069_033_14-2` (069_k05_vegeta150): 636 / 1,052, **position / facing** - Some((636, "pos: #4 sprite 256: (313.421875, 193.76953125) vs game (313.421875, 195.76953125)"))
* `r069_041_14-2` (069_k05_vegeta150): 480 / 757, **position / facing** - Some((480, "pos: #4 sprite 256: (200, 178.484375) vs game (200, 180.484375)"))
* `r069_047_14-2` (069_k05_vegeta150): 523 / 1,070, **position / facing** - Some((523, "dir: #3 sprite 256: 3 vs game 2"))
* `r069_049_14-2` (069_k05_vegeta150): 565 / 1,192, **position / facing** - Some((565, "pos: #4 sprite 256: (200, 200) vs game (200, 198)"))
* `r069_053_14-2` (069_k05_vegeta150): 1,088 / 1,453, **RNG** - Some((1088, "rng: [f43fb9d4, 9f089e19] vs game [6803e85c, 96742087]"))
* `r069_055_14-2` (069_k05_vegeta150): 840 / 1,640, **position / facing** - Some((840, "pos: #4 sprite 256: (239.32421875, 185.39453125) vs game (239.32421875, 183.39453125)"))
* `r069_057_14-2` (069_k05_vegeta150): 650 / 1,033, **position / facing** - Some((650, "dir: #4 sprite 256: 3 vs game 2"))
* `r069_059_14-2` (069_k05_vegeta150): 331 / 1,154, **RNG** - Some((331, "rng: [4dd70c52, 2bff9164] vs game [7b5d59a7, 1d723a97]"))
* `r069_073_14-2` (069_k05_vegeta150): 710 / 993, **RNG** - Some((710, "rng: [7a0a86a1, 0aab3e3a] vs game [767d7d28, 2df1b4f7]"))
* `r069_077_14-2` (069_k05_vegeta150): 246 / 1,666, **RNG** - Some((246, "rng: [25d9021b, ee0d7ff5] vs game [c6a9c179, b03716c7]"))
* `r069_085_14-2` (069_k05_vegeta150): 622 / 1,346, **RNG** - Some((622, "rng: [f8327a8d, a5592521] vs game [5fa713df, 6f5b12c3]"))
* `r069_087_14-2` (069_k05_vegeta150): 540 / 949, **position / facing** - Some((540, "pos: #4 sprite 256: (200, 200) vs game (200, 198)"))
* `r069_089_14-2` (069_k05_vegeta150): 331 / 1,462, **RNG** - Some((331, "rng: [4e913927, 916a3cd0] vs game [c036c948, 51b277db]"))
* `r069_101_14-2` (069_k05_vegeta150): 311 / 1,406, **position / facing** - Some((311, "pos: #3 sprite 256: (79.37890625, 105.875) vs game (77.37890625, 105.875)"))
* `r069_117_14-2` (069_k05_vegeta150): 738 / 1,793, **position / facing** - Some((738, "pos: #4 sprite 256: (247.90625, 198.87109375) vs game (248.703125, 198.89453125)"))
* `r069_125_14-2` (069_k05_vegeta150): 87 / 2,012, **RNG** - Some((87, "rng: [5f0fda5a, 018eaccf] vs game [60553a04, 72fd0483]"))
* `r069_135_14-2` (069_k05_vegeta150): 557 / 876, **RNG** - Some((557, "rng: [e48a2fee, fcdd03b3] vs game [1b88efec, f6921a42]"))
* `r069_139_14-2` (069_k05_vegeta150): 225 / 1,252, **position / facing** - Some((225, "pos: #3 sprite 256: (88.24609375, 142.53125) vs game (88.24609375, 143.328125)"))
* `r069_143_14-2` (069_k05_vegeta150): 633 / 937, **position / facing** - Some((633, "pos: #4 sprite 256: (202.390625, 180.24609375) vs game (203.1875, 180.30078125)"))
* `r069_151_14-2` (069_k05_vegeta150): 155 / 885, **position / facing** - Some((155, "pos: #3 sprite 256: (89.7265625, 131.375) vs game (89.66796875, 132.171875)"))
* `r069_161_14-2` (069_k05_vegeta150): 166 / 2,174, **position / facing** - Some((166, "pos: #3 sprite 256: (89.8203125, 130.578125) vs game (91.8203125, 130.578125)"))
* `r069_167_14-2` (069_k05_vegeta150): 623 / 914, **RNG** - Some((623, "rng: [4f5723e8, edd9719e] vs game [bffb91c7, cb05f4c1]"))

Update 2 (A): **`return_to_field` cuts import** (mid-area: the RAM's actors, AI stacks / runtimes, grids, camera;
`engine/docs/segments.md` §6). Kid Buu run: 9 / 14 cuts exact, 8 gated (`rkb_*`). Re-cut with `--events`. Still
refused: scene loads 0x72 .. 0x74, `return_to_field` onto the 3D map.
