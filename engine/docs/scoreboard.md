# Parity scoreboard: the real playthrough on wf-core

Tree: `9a26dfd` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
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
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,646 | 8,440 | 371 | 367 | 380,206 | 356,848 | 93.9 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 252 | 187,291 | 185,309 | 98.9 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 443 | 467,701 | 413,943 | 88.5 |
| **all** |  | 1,668,050 | 45,294 | 1519 | 1505 | 1,622,756 | 1,537,000 | 94.7 |

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| menu timing | Earth agent (redraw / mixer timing) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| RNG | chapter agent (who draws first: enemy AI / hits) | 0 | 1 | 0 | 1 | 0 | 0 | 2 |
| screen fade | cutscene / script-host agent | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| camera | chapter agent (camera scroll) | 0 | 0 | 0 | 1 | 1 | 2 | 4 |
| animation | chapter agent | 0 | 1 | 0 | 1 | 0 | 0 | 2 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 0 | 0 | 3 | 3 |
| object set | chapter agent (spawns / removal) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 5 segments, 53,758 frames lost

* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r052_006_6-9` (052_m_gotenks, position / facing): 18,721 lost - Some((5768, "hp: 470 vs game 488"))
* `r052_004_1-5` (052_m_gotenks, camera): 8,527 lost - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r067_071_14-2` (067_k03_goku158, camera): 1,722 lost - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
* `r064_004_21-6` (064_m_kid_buu_saga, position / facing): 787 lost - Some((254, "pos: #6 sprite 87: (112.26953125, 36.49609375) vs game (112.26953125, 35.99609375)"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 3 segments, 22,803 frames lost

* `r033_002_6-1` (033_x05_s026, animation): 12,814 lost - Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8")) **Known:** Goku vs Majin Buu, 0-21 (fixed: the hit's screen shake ticks through the pause fade-out, 9,545; a pause menu's frames advance the clock 0x03000F94, so Buu's on-hit IfTimeInState(90) is true after a heal menu, 9,904 - not a battle rule). Now at 24,002 (an animation cel)
* `r002_006_4-1` (002_d04_bandana, camera): 9,740 lost - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: a patrol airship's s509 starts a frame later, 1,763; an on-defeat unit callback's prop show in its first run finishes a frame later, 4,178). Now at 10,623 (camera)
* `r017_009_13-45` (017_db6_fukurou, RNG): 249 lost - Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]"))

**Earth agent** - 2 segments, 6,658 frames lost

* `rE2__090_7-3` (E2_earth, RNG): 5,989 lost - Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]")) **Known:** not the Earth agent's e_7_3_1: the real run's Earth chapter cut at its last importable entry, Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys' last 51,940 (no importable entry after it) and ~480 of the ship route's up to its first entry; first divergence at its frame 6,658: actor #42 (sprite 222) raw 1 cel 0 vs the game's cel 1, tick 1776 vs 1777 - an NPC idle animation one tick behind after ~1,800 frames
* `rE2__089_7-2` (E2_earth, animation): 669 lost - Some((1601, "anim: #39 sprite 112: raw 1 cel 1 vs game raw 1 cel 0 tick 75"))

**side-content agent** - 2 segments, 1,301 frames lost

* `r047_002_18-13` (047_h_h13, object set): 1,103 lost - Some((1701, "missing: obj #16 sprite 117 has no entity")) **Known:** heal menu / Switch Character in 18-13: one frame short
* `r038_001_18-1` (038_j03_goku120, camera): 198 lost - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))

**Earth agent (redraw / mixer timing)** - 1 segments, 681 frames lost

* `r044_015_18-1` (044_h_back1, menu timing): 681 lost - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)

**cutscene / script-host agent** - 1 segments, 555 frames lost

* `r012_014_12-19` (012_db5_a01, screen fade): 555 lost - Some((2567, "fade: black true vs game false"))


## Where long segments really diverge (every-N board)

From `engine/docs/scoreboard-mid.md` (tree `9a26dfd`, tiles of 1,000 frames; the whole run 93.8% verified exact). Each
divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next
cut. A divergence at a tile's frame 0 is usually the any-frame import, not the game rule - say so to the importer
agent.

**earth** (Earth agent) - 16 divergences inside tiles, 12,149 frames

* `rE2__090_7-3` frame 4,005 (sprite memory): 995 lost - Some((5, "vram: used blocks (start, size) ours only [(56, 1)], game only []"))
* `rE2__090_7-3` frame 8,005 (RNG): 995 lost - Some((5, "rng: [3bb9ca68, 742af456] vs game [cfca0a3c, e92c0cc5]"))
* `rE2__090_7-3` frame 19,007 (sprite memory): 993 lost - Some((7, "vram: used blocks (start, size) ours only [(285, 4)], game only []"))
* `rE2__090_7-3` frame 29,008 (RNG): 992 lost - Some((8, "rng: [9cb66103, 9e07d4db] vs game [f3a53164, 367da352]"))
* `rE2__090_7-3` frame 38,026 (script / text, cutscene / script-host agent): 974 lost - Some((26, "text: game opens a text box: true, ours: false"))

**fat** (dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)) - 20 divergences inside tiles, 16,466 frames

* `r033_002_6-1` frame 18,001 (sprite memory): 999 lost - Some((1, "vram: used blocks (start, size) ours only [(316, 1)], game only []"))
* `r033_002_6-1` frame 8,008 (sprite memory): 992 lost - Some((8, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
* `r033_002_6-1` frame 10,009 (sprite memory): 991 lost - Some((9, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
* `r033_002_6-1` frame 27,012 (sprite memory): 988 lost - Some((12, "vram: used blocks (start, size) ours only [(379, 4)], game only []"))
* `r033_002_6-1` frame 11,013 (sprite memory): 987 lost - Some((13, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))

**superbuu** (Super Buu agent) - 32 divergences inside tiles, 26,360 frames

* `r062_001_20-6` frame 9,002 (sprite memory): 998 lost - Some((2, "vram: used blocks (start, size) ours only [(257, 64)], game only []"))
* `r062_001_20-6` frame 17,004 (RNG): 996 lost - Some((4, "rng: [aef64cec, 580be19f] vs game [e9c9e8c2, 2f580dcb]"))
* `r052_006_6-9` frame 19,005 (object set): 995 lost - Some((5, "missing: obj #5 sprite 110 has no entity (ours there: sprite Some(108))"))
* `r062_001_20-6` frame 14,005 (sound, Earth agent (sound timing) / audio): 995 lost - Some((5, "snd: [] vs game [\"play 34 64\", \"stop 3\"]"))
* `r062_001_20-6` frame 11,006 (sprite memory): 994 lost - Some((6, "vram: used blocks (start, size) ours only [], game only [(377, 8)]"))

**importer** (any-frame import) - tiles that differ at their first frame, by kind:

* position / facing `pos: sprite : (, ) vs game`: 9 tiles, 9,000 frames (earth, fat, superbuu)
* sprite memory `vram: used blocks (start, size) ours onl`: 7 tiles, 7,000 frames (earth, fat, superbuu)
* camera `cam: (, ) vs game`: 7 tiles, 5,799 frames (earth, fat, superbuu)
* screen fade `fade: black true vs game false`: 4 tiles, 4,000 frames (earth, fat, superbuu)
* position / facing `dir: sprite :  vs game`: 2 tiles, 1,303 frames (fat)
* RNG `rng: [feb, be] vs game [ccbaa, aeebf]`: 1 tiles, 1,000 frames (fat)
* animation `anim: sprite : raw  cel  tick  vs game r`: 1 tiles, 1,000 frames (fat)
* animation `anim: sprite : raw  cel  vs game raw  ce`: 1 tiles, 1,000 frames (superbuu)
* position / facing `hp:  vs game`: 1 tiles, 1,000 frames (earth)
* RNG `rng: [dea, bac] vs game [bcc, ddfce]`: 1 tiles, 1,000 frames (earth)


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r052_006_6-9` | 052_m_gotenks | position / facing | 18,721 | Some((5768, "hp: 470 vs game 488")) |
| `r033_002_6-1` | 033_x05_s026 | animation | 12,814 | Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8")) |
| `r002_006_4-1` | 002_d04_bandana | camera | 9,740 | Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) |
| `r052_004_1-5` | 052_m_gotenks | camera | 8,527 | Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)")) |
| `rE2__090_7-3` | E2_earth | RNG | 5,989 | Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |
| `r047_002_18-13` | 047_h_h13 | object set | 1,103 | Some((1701, "missing: obj #16 sprite 117 has no entity")) |
| `r064_004_21-6` | 064_m_kid_buu_saga | position / facing | 787 | Some((254, "pos: #6 sprite 87: (112.26953125, 36.49609375) vs game (112.26953125, 35.99609375)")) |
| `r044_015_18-1` | 044_h_back1 | menu timing | 681 | Some((577, "fade: black true vs game false")) |
| `rE2__089_7-2` | E2_earth | animation | 669 | Some((1601, "anim: #39 sprite 112: raw 1 cel 1 vs game raw 1 cel 0 tick 75")) |
| `r012_014_12-19` | 012_db5_a01 | screen fade | 555 | Some((2567, "fade: black true vs game false")) |
| `r017_009_13-45` | 017_db6_fukurou | RNG | 249 | Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]")) |
| `r038_001_18-1` | 038_j03_goku120 | camera | 198 | Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)")) |

## Recurring first divergences (3+ segments)

* 4 x `cam`: `r002_006_4-1`, `r038_001_18-1`, `r052_004_1-5`, `r067_071_14-2`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth

* `rE2__089_7-2` (E2_earth): 1,601 / 2,270, **animation** - Some((1601, "anim: #39 sprite 112: raw 1 cel 1 vs game raw 1 cel 0 tick 75"))
* `rE2__090_7-3` (E2_earth): 46,426 / 52,415, **RNG** - Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]")) **Known:** not the Earth agent's e_7_3_1: the real run's Earth chapter cut at its last importable entry, Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys' last 51,940 (no importable entry after it) and ~480 of the ship route's up to its first entry; first divergence at its frame 6,658: actor #42 (sprite 222) raw 1 cel 0 vs the game's cel 1, tick 1776 vs 1777 - an NPC idle animation one tick behind after ~1,800 frames

### ship


### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336, **camera** - Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) **Known:** West City -> the Bandana hunt (fixed: a patrol airship's s509 starts a frame later, 1,763; an on-defeat unit callback's prop show in its first run finishes a frame later, 4,178). Now at 10,623 (camera)
* `r012_014_12-19` (012_db5_a01): 2,567 / 3,122, **screen fade** - Some((2567, "fade: black true vs game false"))
* `r017_009_13-45` (017_db6_fukurou): 270 / 519, **RNG** - Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]"))
* `r033_002_6-1` (033_x05_s026): 24,002 / 36,816, **animation** - Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8")) **Known:** Goku vs Majin Buu, 0-21 (fixed: the hit's screen shake ticks through the pause fade-out, 9,545; a pause menu's frames advance the clock 0x03000F94, so Buu's on-hit IfTimeInState(90) is true after a heal menu, 9,904 - not a battle rule). Now at 24,002 (an animation cel)

### janemba

* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r044_015_18-1` (044_h_back1): 577 / 1,258, **menu timing** - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)
* `r047_002_18-13` (047_h_h13): 1,699 / 2,802, **object set** - Some((1701, "missing: obj #16 sprite 117 has no entity")) **Known:** heal menu / Switch Character in 18-13: one frame short

### superbuu

* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084, **camera** - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r052_006_6-9` (052_m_gotenks): 5,768 / 24,489, **position / facing** - Some((5768, "hp: 470 vs game 488"))
* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r064_004_21-6` (064_m_kid_buu_saga): 254 / 1,041, **position / facing** - Some((254, "pos: #6 sprite 87: (112.26953125, 36.49609375) vs game (112.26953125, 35.99609375)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
