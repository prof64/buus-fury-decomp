# Parity scoreboard: the real playthrough on wf-core

Tree: `f11daf9` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
(the cuts: `scoreboard.py cut`, local under `work/real/`; the run itself: `tools/segments/realrun.py`, §"How to replay
the real run" in `engine/docs/segments.md`). A segment = the real game from one importable area entry (an exit
arrival with nothing running) to the next; an entry by a script warp / in a cutscene is not a cut - its frames
stay in the segment before it, and a segment runs on across the boundaries of the run's key logs (`scoreboard.py
extend`). "not cut" = frames in no segment (the run's start up to its first importable entry) - a chapter's
frames are counted where its segments start.

| chapter | owner | run frames | not cut | segments | exact | frames | matched | % |
|---|---|---|---|---|---|---|---|---|
| opening | Earth agent (title, New Game, Other World) | 42,275 | 7,898 | 17 | 17 | 34,377 | 34,377 | 100.0 |
| earth | Earth agent | 160,503 | 2,938 | 90 | 88 | 157,565 | 150,907 | 95.8 |
| ship | ship agent | 398,194 | 2,578 | 338 | 338 | 395,616 | 395,616 | 100.0 |
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,646 | 8,440 | 371 | 364 | 380,206 | 345,353 | 90.8 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 251 | 187,291 | 184,661 | 98.6 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 436 | 467,701 | 351,864 | 75.2 |
| **all** |  | 1,668,050 | 45,294 | 1519 | 1494 | 1,622,756 | 1,462,778 | 90.1 |

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| sound | Earth agent (sound timing) / audio | 0 | 0 | 0 | 0 | 1 | 1 | 2 |
| menu timing | Earth agent (redraw / mixer timing) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| RNG | chapter agent (who draws first: enemy AI / hits) | 0 | 1 | 0 | 2 | 0 | 2 | 5 |
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| camera | chapter agent (camera scroll) | 0 | 0 | 0 | 3 | 1 | 3 | 7 |
| animation | chapter agent | 0 | 1 | 0 | 1 | 0 | 0 | 2 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 0 | 1 | 6 | 7 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 11 segments, 104,646 frames lost

* `r069_168_14-1` (069_k05_vegeta150, position / facing): 35,663 lost - Some((20168, "pos: #271 sprite 47: (268.046875, 210.015625) vs game (268.046875, 210.1484375)")) **Known:** Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore's black lasts 26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the 26th frame)
* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r052_006_6-9` (052_m_gotenks, RNG): 19,801 lost - Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]"))
* `r052_004_1-5` (052_m_gotenks, camera): 8,527 lost - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r064_029_21-26` (064_m_kid_buu_saga, camera): 7,777 lost - Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 6 segments, 34,298 frames lost

* `r033_002_6-1` (033_x05_s026, animation): 12,814 lost - Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8")) **Known:** Goku vs Majin Buu, 0-21 (fixed: the hit's screen shake ticks through the pause fade-out, 9,545; a pause menu's frames advance the clock 0x03000F94, so Buu's on-hit IfTimeInState(90) is true after a heal menu, 9,904 - not a battle rule). Now at 24,002 (an animation cel)
* `r002_006_4-1` (002_d04_bandana, camera): 9,740 lost - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: a patrol airship's s509 starts a frame later, 1,763; an on-defeat unit callback's prop show in its first run finishes a frame later, 4,178). Now at 10,623 (camera)
* `r009_006_4-1` (009_d16_desert, camera): 9,089 lost - Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)"))
* `r012_020_12-27` (012_db5_a01, RNG): 1,555 lost - Some((1856, "rng: [942926ee, 25aeea64] vs game [de77fbf0, 756a9e8f]"))
* `r002_004_4-1` (002_d04_bandana, camera): 851 lost - Some((741, "cam: (37.5556640625, 512.0166015625) vs game (37.642578125, 512.0224609375)"))

**Earth agent (sound timing) / audio** - 2 segments, 12,686 frames lost

* `r061_004_20-5` (061_g05_goku141, sound): 11,191 lost - Some((5251, "snd: [] vs game [\"play 41 32\"]"))
* `r049_001_18-14` (049_j_sj2, sound): 1,495 lost - Some((7628, "snd: [] vs game [\"play 41 32\"]"))

**Earth agent** - 2 segments, 6,658 frames lost

* `rE2__090_7-3` (E2_earth, RNG): 5,989 lost - Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]")) **Known:** not the Earth agent's e_7_3_1: the real run's Earth chapter cut at its last importable entry, Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys' last 51,940 (no importable entry after it) and ~480 of the ship route's up to its first entry; first divergence at its frame 6,658: actor #42 (sprite 222) raw 1 cel 0 vs the game's cel 1, tick 1776 vs 1777 - an NPC idle animation one tick behind after ~1,800 frames
* `rE2__089_7-2` (E2_earth, animation): 669 lost - Some((1601, "anim: #39 sprite 112: raw 1 cel 1 vs game raw 1 cel 0 tick 75"))

**Earth agent (redraw / mixer timing)** - 1 segments, 681 frames lost

* `r044_015_18-1` (044_h_back1, menu timing): 681 lost - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

**cutscene / script-host agent** - 1 segments, 555 frames lost

* `r012_014_12-19` (012_db5_a01, screen fade): 555 lost - Some((2567, "fade: black true vs game false"))

**side-content agent** - 2 segments, 454 frames lost

* `r042_001_18-11` (042_h_statues, position / facing): 256 lost - Some((9672, "pos: #5 sprite 151: (144, 208) vs game (177, 132)"))
* `r038_001_18-1` (038_j03_goku120, camera): 198 lost - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `f11daf9`, tiles of 1,000 frames; the whole run 89.6% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**earth** (Earth agent) - 17 divergences inside tiles, 13,128 frames

* `rE2__090_7-3` frame 4,005 (sprite memory): 995 lost - Some((5, "vram: used blocks (start, size) ours only [(56, 1)], game only []"))
* `rE2__090_7-3` frame 19,007 (sprite memory): 993 lost - Some((7, "vram: used blocks (start, size) ours only [(285, 4)], game only []"))
* `rE2__090_7-3` frame 29,008 (RNG): 992 lost - Some((8, "rng: [9cb66103, 9e07d4db] vs game [f3a53164, 367da352]"))
* `rE2__090_7-3` frame 8,013 (RNG): 987 lost - Some((13, "rng: [1de73f0b, 43d27031] vs game [63608b4d, 47a6d2d4]"))
* `rE2__090_7-3` frame 45,013 (object set): 987 lost - Some((13, "missing: obj #15 sprite 252 has no entity"))

**ship** (ship agent) - 5 divergences inside tiles, 4,103 frames

* `r000_002_7-2` frame 1,009 (animation): 991 lost - Some((9, "anim: #2 sprite 138: raw 13 cel 0 tick 16 vs game raw 13 cel 1 tick 16"))
* `r000_002_7-2` frame 17,031 (sprite memory): 969 lost - Some((31, "vram: used blocks (start, size) ours only [], game only [(81, 32), (113, 64), (177, 64), (241, 64)]"))
* `r000_002_7-2` frame 8,068 (animation): 932 lost - Some((68, "anim: #0 sprite 114: raw 21 cel 1 vs game raw 21 cel 0 tick 95"))
* `r000_002_7-2` frame 2,206 (sprite memory): 794 lost - Some((230, "vram: used blocks (start, size) ours only [(409, 4)], game only []"))
* `r000_002_7-2` frame 7,583 (RNG): 417 lost - Some((583, "rng: [514fca98, ee12e8e7] vs game [ba096fee, ccc370a0]"))

**fat** (dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)) - 27 divergences inside tiles, 23,127 frames

* `r002_006_4-1` frame 19,001 (sprite memory): 999 lost - Some((1, "vram: used blocks (start, size) ours only [(247, 16)], game only []"))
* `r033_002_6-1` frame 18,001 (sprite memory): 999 lost - Some((1, "vram: used blocks (start, size) ours only [(316, 1)], game only []"))
* `r002_006_4-1` frame 4,005 (sprite memory): 995 lost - Some((5, "vram: used blocks (start, size) ours only [(263, 16)], game only []"))
* `r033_002_6-1` frame 8,008 (sprite memory): 992 lost - Some((8, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
* `r033_002_6-1` frame 10,009 (sprite memory): 991 lost - Some((9, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))

**janemba** (side-content agent) - 6 divergences inside tiles, 5,349 frames

* `r049_001_18-14` frame 4,003 (RNG): 997 lost - Some((3, "rng: [cafd6caa, 9823db58] vs game [55eaf243, 92bb4090]"))
* `r049_001_18-14` frame 1,005 (position / facing): 995 lost - Some((5, "pos: #2 sprite 147: (116, 68) vs game (116, 67)"))
* `r049_001_18-14` frame 2,006 (position / facing): 994 lost - Some((6, "pos: #2 sprite 147: (116, 67) vs game (116, 68)"))
* `r049_001_18-14` frame 3,006 (position / facing): 994 lost - Some((6, "pos: #2 sprite 147: (116, 67) vs game (116, 68)"))
* `r049_001_18-14` frame 6,036 (sprite memory): 964 lost - Some((36, "vram: used blocks (start, size) ours only [(446, 1)], game only []"))

**superbuu** (Super Buu agent) - 51 divergences inside tiles, 34,937 frames

* `r052_006_6-9` frame 16,001 (position / facing): 999 lost - Some((1, "dir: #5 sprite 49: 3 vs game 1"))
* `r062_001_20-6` frame 9,002 (sprite memory): 998 lost - Some((2, "vram: used blocks (start, size) ours only [(257, 64)], game only []"))
* `r062_001_20-6` frame 17,004 (RNG): 996 lost - Some((4, "rng: [aef64cec, 580be19f] vs game [e9c9e8c2, 2f580dcb]"))
* `r052_006_6-9` frame 19,005 (object set): 995 lost - Some((5, "missing: obj #5 sprite 110 has no entity (ours there: sprite Some(108))"))
* `r069_168_14-1` frame 14,005 (sprite memory): 995 lost - Some((5, "vram: used blocks (start, size) ours only [(349, 4)], game only []"))

**importer** (any-frame import) - tiles that differ at their first frame, by kind:

* position / facing `pos: sprite : (, ) vs game`: 35 tiles, 35,000 frames (earth, fat, superbuu)
* sprite memory `vram: used blocks (start, size) ours onl`: 9 tiles, 9,000 frames (earth, fat, superbuu)
* camera `cam: (, ) vs game`: 10 tiles, 8,931 frames (earth, ship, superbuu)
* animation `anim: sprite : raw  cel  tick  vs game r`: 8 tiles, 8,000 frames (fat, janemba, ship, superbuu)
* animation `anim: sprite : raw  cel  vs game raw  ce`: 4 tiles, 4,000 frames (superbuu)
* position / facing `dir: sprite :  vs game`: 3 tiles, 3,000 frames (fat)
* screen fade `fade: black true vs game false`: 2 tiles, 2,000 frames (superbuu)
* object set `missing: obj sprite  has no entity`: 2 tiles, 2,000 frames (superbuu)
* RNG `rng: [feb, be] vs game [ccbaa, aeebf]`: 1 tiles, 1,000 frames (fat)
* RNG `rng: [bcecf, dfbf] vs game [eafe, da]`: 1 tiles, 1,000 frames (superbuu)


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r069_168_14-1` | 069_k05_vegeta150 | position / facing | 35,663 | Some((20168, "pos: #271 sprite 47: (268.046875, 210.015625) vs game (268.046875, 210.1484375)")) |
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r052_006_6-9` | 052_m_gotenks | RNG | 19,801 | Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]")) |
| `r033_002_6-1` | 033_x05_s026 | animation | 12,814 | Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8")) |
| `r061_004_20-5` | 061_g05_goku141 | sound | 11,191 | Some((5251, "snd: [] vs game [\"play 41 32\"]")) |
| `r002_006_4-1` | 002_d04_bandana | camera | 9,740 | Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) |
| `r009_006_4-1` | 009_d16_desert | camera | 9,089 | Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)")) |
| `r052_004_1-5` | 052_m_gotenks | camera | 8,527 | Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)")) |
| `r064_029_21-26` | 064_m_kid_buu_saga | camera | 7,777 | Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)")) |
| `rE2__090_7-3` | E2_earth | RNG | 5,989 | Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]")) |
| `r057_066_14-1` | 057_g03_gohan141 | position / facing | 3,800 | Some((3527, "pos: #243 sprite 49: (270.41015625, 164.78125) vs game (271.91015625, 164.86328125)")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |
| `r012_020_12-27` | 012_db5_a01 | RNG | 1,555 | Some((1856, "rng: [942926ee, 25aeea64] vs game [de77fbf0, 756a9e8f]")) |
| `r049_001_18-14` | 049_j_sj2 | sound | 1,495 | Some((7628, "snd: [] vs game [\"play 41 32\"]")) |
| `r069_077_14-2` | 069_k05_vegeta150 | RNG | 1,402 | Some((264, "rng: [79a2b111, e7df17bd] vs game [9bb17a3f, de9aefc1]")) |

## Recurring first divergences (3+ segments)

* 7 x `cam`: `r002_004_4-1`, `r002_006_4-1`, `r009_006_4-1`, `r038_001_18-1`, `r052_004_1-5`, `r064_029_21-26`, `r067_071_14-2`
* 5 x `rng`: `r012_020_12-27`, `r017_009_13-45`, `r052_006_6-9`, `r069_077_14-2`, `rE2__090_7-3`
* 3 x `pos sprite 280`: `r062_001_20-6`, `r069_101_14-2`, `r069_139_14-2`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth

* `rE2__089_7-2` (E2_earth): 1,601 / 2,270, **animation** - Some((1601, "anim: #39 sprite 112: raw 1 cel 1 vs game raw 1 cel 0 tick 75"))
* `rE2__090_7-3` (E2_earth): 46,426 / 52,415, **RNG** - Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]")) **Known:** not the Earth agent's e_7_3_1: the real run's Earth chapter cut at its last importable entry, Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys' last 51,940 (no importable entry after it) and ~480 of the ship route's up to its first entry; first divergence at its frame 6,658: actor #42 (sprite 222) raw 1 cel 0 vs the game's cel 1, tick 1776 vs 1777 - an NPC idle animation one tick behind after ~1,800 frames

### ship


### fat

* `r002_004_4-1` (002_d04_bandana): 679 / 1,530, **camera** - Some((741, "cam: (37.5556640625, 512.0166015625) vs game (37.642578125, 512.0224609375)"))
* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336, **camera** - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: a patrol airship's s509 starts a frame later, 1,763; an on-defeat unit callback's prop show in its first run finishes a frame later, 4,178). Now at 10,623 (camera)
* `r009_006_4-1` (009_d16_desert): 396 / 9,485, **camera** - Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)"))
* `r012_014_12-19` (012_db5_a01): 2,567 / 3,122, **screen fade** - Some((2567, "fade: black true vs game false"))
* `r012_020_12-27` (012_db5_a01): 1,856 / 3,411, **RNG** - Some((1856, "rng: [942926ee, 25aeea64] vs game [de77fbf0, 756a9e8f]"))
* `r017_009_13-45` (017_db6_fukurou): 270 / 519, **RNG** - Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]"))
* `r033_002_6-1` (033_x05_s026): 24,002 / 36,816, **animation** - Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8")) **Known:** Goku vs Majin Buu, 0-21 (fixed: the hit's screen shake ticks through the pause fade-out, 9,545; a pause menu's frames advance the clock 0x03000F94, so Buu's on-hit IfTimeInState(90) is true after a heal menu, 9,904 - not a battle rule). Now at 24,002 (an animation cel)

### janemba

* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r042_001_18-11` (042_h_statues): 9,672 / 9,928, **position / facing** - Some((9672, "pos: #5 sprite 151: (144, 208) vs game (177, 132)"))
* `r044_015_18-1` (044_h_back1): 577 / 1,258, **menu timing** - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)
* `r049_001_18-14` (049_j_sj2): 7,628 / 9,123, **sound** - Some((7628, "snd: [] vs game [\"play 41 32\"]"))

### superbuu

* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084, **camera** - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r052_006_6-9` (052_m_gotenks): 4,688 / 24,489, **RNG** - Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]"))
* `r057_066_14-1` (057_g03_gohan141): 3,527 / 7,327, **position / facing** - Some((3527, "pos: #243 sprite 49: (270.41015625, 164.78125) vs game (271.91015625, 164.86328125)"))
* `r061_004_20-5` (061_g05_goku141): 5,251 / 16,442, **sound** - Some((5251, "snd: [] vs game [\"play 41 32\"]"))
* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r064_004_21-6` (064_m_kid_buu_saga): 254 / 1,041, **position / facing** - Some((254, "pos: #6 sprite 87: (112.26953125, 36.49609375) vs game (112.26953125, 35.99609375)"))
* `r064_029_21-26` (064_m_kid_buu_saga): 12,723 / 20,500, **camera** - Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
* `r069_077_14-2` (069_k05_vegeta150): 264 / 1,666, **RNG** - Some((264, "rng: [79a2b111, e7df17bd] vs game [9bb17a3f, de9aefc1]"))
* `r069_101_14-2` (069_k05_vegeta150): 862 / 1,406, **position / facing** - Some((862, "pos: #5 sprite 280: (176.57421875, 184.94140625) vs game (175.82421875, 184.94140625)"))
* `r069_139_14-2` (069_k05_vegeta150): 630 / 1,252, **position / facing** - Some((630, "pos: #5 sprite 280: (162.046875, 176) vs game (161.296875, 176)"))
* `r069_168_14-1` (069_k05_vegeta150): 20,168 / 55,831, **position / facing** - Some((20168, "pos: #271 sprite 47: (268.046875, 210.015625) vs game (268.046875, 210.1484375)")) **Known:** Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore's black lasts 26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the 26th frame)
