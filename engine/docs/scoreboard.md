# Parity scoreboard: the real playthrough on wf-core

Tree: `f1a156b` + uncommitted engine changes. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`
(the cuts: `scoreboard.py cut`, local under `work/real/`; the run itself: `tools/segments/realrun.py`, §"How to replay
the real run" in `engine/docs/segments.md`). A segment = the real game from one importable area entry (an exit
arrival with nothing running) to the next; an entry by a script warp / in a cutscene is not a cut - its frames
stay in the segment before it, and a segment runs on across the boundaries of the run's key logs (`scoreboard.py
extend`). "not cut" = frames in no segment (the run's start up to its first importable entry) - a chapter's
frames are counted where its segments start.

| chapter | owner | run frames | not cut | segments | exact | frames | matched | % |
|---|---|---|---|---|---|---|---|---|
| opening | Earth agent (title, New Game, Other World) | 42,275 | 7,898 | 17 | 17 | 34,377 | 34,377 | 100.0 |
| earth | Earth agent | 160,503 | 2,938 | 90 | 88 | 157,565 | 110,970 | 70.4 |
| ship | ship agent | 398,194 | 2,578 | 338 | 337 | 395,616 | 394,207 | 99.6 |
| fat | dungeon agents A / B (+ ship agent: Thieves Den, Pilaf) | 388,646 | 8,440 | 371 | 362 | 380,206 | 322,469 | 84.8 |
| janemba | side-content agent | 199,703 | 12,412 | 255 | 245 | 187,291 | 174,987 | 93.4 |
| superbuu | Super Buu agent | 478,729 | 11,028 | 448 | 401 | 467,701 | 307,986 | 65.9 |
| **all** |  | 1,668,050 | 45,294 | 1519 | 1450 | 1,622,756 | 1,344,996 | 82.9 |

## Non-exact segments by category

| category | owner | opening | earth | ship | fat | janemba | superbuu | all |
|---|---|---|---|---|---|---|---|---|
| sound | Earth agent (sound timing) / audio | 0 | 0 | 0 | 0 | 1 | 1 | 2 |
| menu timing | Earth agent (redraw / mixer timing) | 0 | 0 | 0 | 0 | 2 | 2 | 4 |
| RNG | chapter agent (who draws first: enemy AI / hits) | 0 | 0 | 0 | 3 | 3 | 16 | 22 |
| camera | chapter agent (camera scroll) | 0 | 0 | 1 | 3 | 1 | 5 | 10 |
| animation | chapter agent | 0 | 2 | 0 | 0 | 0 | 0 | 2 |
| position / facing | chapter agent (enemy AI / knockback) | 0 | 0 | 0 | 3 | 2 | 23 | 28 |
| object set | chapter agent (spawns / removal) | 0 | 0 | 0 | 0 | 1 | 0 | 1 |

## Top items per owner

The five biggest losses each owner can take (unmatched frames; the full list below).

**Super Buu agent** - 44 segments, 92,397 frames lost

* `r062_001_20-6` (062_m_inside_buu, position / facing): 24,001 lost - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r052_006_6-9` (052_m_gotenks, RNG): 19,801 lost - Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]"))
* `r052_004_1-5` (052_m_gotenks, camera): 8,527 lost - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r064_029_21-26` (064_m_kid_buu_saga, camera): 7,777 lost - Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)"))
* `r057_066_14-1` (057_g03_gohan141, RNG): 3,824 lost - Some((3503, "rng: [2203fea3, a2a3ad3b] vs game [140b8563, e41e1348]"))

**dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)** - 9 segments, 57,737 frames lost

* `r033_002_6-1` (033_x05_s026, position / facing): 26,912 lost - Some((9904, "pos: #129 sprite 43: (25.0078125, 184.9453125) vs game (25.0390625, 184.80078125)")) **Known:** Goku vs Majin Buu, 0-21 (was an RNG row at 9,545: the hit's screen shake now ticks through the pause fade-out). Now at 9,904: one punch's draws come in a different order - game: crit chance, rand(26), two chances (0x080192AB: the hit tree), the punch sfx / anim rands (0x080116DD), then the knockback rands (0x08018B4B: KnockbackSlide enter); ours: one hit-tree chance and the knockback before the punch rands - same count, different values (knock (-152, 44) vs (-144, 7))
* `r002_006_4-1` (002_d04_bandana, RNG): 16,158 lost - Some((4178, "rng: [8b4fc115, a32823a5] vs game [ac3da3d0, 2446a154]")) **Known:** West City -> the Bandana hunt (was an animation row at 1,763: a patrol airship's s509 now starts a frame later, field.rs trigger_self). Now at 4,178 (0-55): the bandits' on-defeat bytecode 0x08073EFC (run_script(506) -> prop_show, then chance(25)) resumes 2 frames after prop_show in the game, 1 in ours; prop_show from every unit script is not it (breaks the take-off at 1,469)
* `r009_006_4-1` (009_d16_desert, camera): 9,089 lost - Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)"))
* `r012_014_12-19` (012_db5_a01, position / facing): 1,629 lost - Some((1493, "pos: #27 sprite 188: (294.08984375, 121.76171875) vs game (294.38671875, 122.05859375)"))
* `r012_020_12-27` (012_db5_a01, RNG): 1,555 lost - Some((1856, "rng: [942926ee, 25aeea64] vs game [de77fbf0, 756a9e8f]"))

**Earth agent (redraw / mixer timing)** - 4 segments, 56,983 frames lost

* `r069_168_14-1` (069_k05_vegeta150, menu timing): 55,418 lost - Some((438, "fade: black false vs game true")) **Known:** Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore's black lasts 26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the 26th frame)
* `r067_136_14-1` (067_k03_goku158, menu timing): 709 lost - Some((426, "fade: black false vs game true")) **Known:** Switch Character at 14-1 (Goku -> Vegeta): game 25 black frames, predicted 24 (46,986 short); the same save point's Trunks <-> Goten switches (r017_037) predict right with margins 86 k / 50 k
* `r044_015_18-1` (044_h_back1, menu timing): 681 lost - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)
* `r038_002_18-2` (038_j03_goku120, menu timing): 175 lost - Some((1563, "fade: black false vs game true")) **Known:** a heal menu closed in 18-2: predicted 21 black frames 208 cycles before the 22nd

**Earth agent** - 2 segments, 46,595 frames lost

* `rE2__090_7-3` (E2_earth, animation): 45,757 lost - Some((6658, "anim: #42 sprite 222: raw 1 cel 0 tick 1776 vs game raw 1 cel 1 tick 1777")) **Known:** not the Earth agent's e_7_3_1: the real run's Earth chapter cut at its last importable entry, Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys' last 51,940 (no importable entry after it) and ~480 of the ship route's up to its first entry; first divergence at its frame 6,658: actor #42 (sprite 222) raw 1 cel 0 vs the game's cel 1, tick 1776 vs 1777 - an NPC idle animation one tick behind after ~1,800 frames
* `rE2__089_7-2` (E2_earth, animation): 838 lost - Some((1432, "anim: #17 sprite 233: raw 1 cel 3 tick 29 vs game raw 1 cel 0 tick 41"))

**Earth agent (sound timing) / audio** - 2 segments, 12,686 frames lost

* `r061_004_20-5` (061_g05_goku141, sound): 11,191 lost - Some((5251, "snd: [] vs game [\"play 41 32\"]"))
* `r049_001_18-14` (049_j_sj2, sound): 1,495 lost - Some((7628, "snd: [] vs game [\"play 41 32\"]"))

**side-content agent** - 7 segments, 9,953 frames lost

* `r040_021_18-10` (040_h_maze, position / facing): 6,977 lost - Some((568, "pos: #95 sprite 229: (225.5, 244) vs game (225, 244)"))
* `r046_045_18-2` (046_h_veg1, RNG): 928 lost - Some((754, "rng: [4f3cec76, d4632e53] vs game [866b86b4, 2f27cc19]"))
* `r036_002_1-5` (036_j01_hfil, RNG): 805 lost - Some((2595, "rng: [c8663cf4, d2722128] vs game [1943d84d, 80803918]"))
* `r038_045_18-1` (038_j03_goku120, object set): 567 lost - Some((251, "missing: obj #1 sprite 151 has no entity"))
* `r042_001_18-11` (042_h_statues, position / facing): 256 lost - Some((9672, "pos: #5 sprite 151: (144, 208) vs game (177, 132)"))

**ship agent** - 1 segments, 1,409 frames lost

* `r000_002_7-2` (000_ship_c14, camera): 1,409 lost - Some((18475, "cam: (37.44140625, 102.95703125) vs game (37.419921875, 102.9599609375)"))


## Biggest losses (unmatched frames)

| segment | stretch | category | lost | first divergence |
|---|---|---|---|---|
| `r069_168_14-1` | 069_k05_vegeta150 | menu timing | 55,418 | Some((438, "fade: black false vs game true")) |
| `rE2__090_7-3` | E2_earth | animation | 45,757 | Some((6658, "anim: #42 sprite 222: raw 1 cel 0 tick 1776 vs game raw 1 cel 1 tick 1777")) |
| `r033_002_6-1` | 033_x05_s026 | position / facing | 26,912 | Some((9904, "pos: #129 sprite 43: (25.0078125, 184.9453125) vs game (25.0390625, 184.80078125)")) |
| `r062_001_20-6` | 062_m_inside_buu | position / facing | 24,001 | Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)")) |
| `r052_006_6-9` | 052_m_gotenks | RNG | 19,801 | Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]")) |
| `r002_006_4-1` | 002_d04_bandana | RNG | 16,158 | Some((4178, "rng: [8b4fc115, a32823a5] vs game [ac3da3d0, 2446a154]")) |
| `r061_004_20-5` | 061_g05_goku141 | sound | 11,191 | Some((5251, "snd: [] vs game [\"play 41 32\"]")) |
| `r009_006_4-1` | 009_d16_desert | camera | 9,089 | Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)")) |
| `r052_004_1-5` | 052_m_gotenks | camera | 8,527 | Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)")) |
| `r064_029_21-26` | 064_m_kid_buu_saga | camera | 7,777 | Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)")) |
| `r040_021_18-10` | 040_h_maze | position / facing | 6,977 | Some((568, "pos: #95 sprite 229: (225.5, 244) vs game (225, 244)")) |
| `r057_066_14-1` | 057_g03_gohan141 | RNG | 3,824 | Some((3503, "rng: [2203fea3, a2a3ad3b] vs game [140b8563, e41e1348]")) |
| `r069_161_14-2` | 069_k05_vegeta150 | position / facing | 2,008 | Some((166, "pos: #3 sprite 256: (89.8203125, 130.578125) vs game (91.8203125, 130.578125)")) |
| `r069_125_14-2` | 069_k05_vegeta150 | RNG | 1,925 | Some((87, "rng: [5f0fda5a, 018eaccf] vs game [60553a04, 72fd0483]")) |
| `r067_071_14-2` | 067_k03_goku158 | camera | 1,722 | Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)")) |

## Recurring first divergences (3+ segments)

* 22 x `rng`: `r002_006_4-1`, `r012_020_12-27`, `r017_009_13-45`, `r036_002_1-5`, `r037_084_18-1`, `r046_045_18-2`, `r052_006_6-9`, `r057_011_14-2` ...
* 19 x `pos sprite 256`: `r057_005_14-2`, `r057_013_14-2`, `r057_015_14-2`, `r057_021_14-2`, `r057_059_14-2`, `r069_003_14-2`, `r069_013_14-2`, `r069_021_14-2` ...
* 10 x `cam`: `r000_002_7-2`, `r002_004_4-1`, `r004_022_9-1`, `r009_006_4-1`, `r038_001_18-1`, `r052_004_1-5`, `r061_017_20-6`, `r061_019_20-6` ...
* 3 x `fade: black false vs game true`: `r038_002_18-2`, `r067_136_14-1`, `r069_168_14-1`

## Every non-exact segment

segment (stretch): matched / frames, category - first divergence

### opening


### earth

* `rE2__089_7-2` (E2_earth): 1,432 / 2,270, **animation** - Some((1432, "anim: #17 sprite 233: raw 1 cel 3 tick 29 vs game raw 1 cel 0 tick 41"))
* `rE2__090_7-3` (E2_earth): 6,658 / 52,415, **animation** - Some((6658, "anim: #42 sprite 222: raw 1 cel 0 tick 1776 vs game raw 1 cel 1 tick 1777")) **Known:** not the Earth agent's e_7_3_1: the real run's Earth chapter cut at its last importable entry, Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys' last 51,940 (no importable entry after it) and ~480 of the ship route's up to its first entry; first divergence at its frame 6,658: actor #42 (sprite 222) raw 1 cel 0 vs the game's cel 1, tick 1776 vs 1777 - an NPC idle animation one tick behind after ~1,800 frames

### ship

* `r000_002_7-2` (000_ship_c14): 18,450 / 19,859, **camera** - Some((18475, "cam: (37.44140625, 102.95703125) vs game (37.419921875, 102.9599609375)"))

### fat

* `r002_004_4-1` (002_d04_bandana): 679 / 1,530, **camera** - Some((741, "cam: (37.5556640625, 512.0166015625) vs game (37.642578125, 512.0224609375)"))
* `r002_006_4-1` (002_d04_bandana): 4,178 / 20,336, **RNG** - Some((4178, "rng: [8b4fc115, a32823a5] vs game [ac3da3d0, 2446a154]")) **Known:** West City -> the Bandana hunt (was an animation row at 1,763: a patrol airship's s509 now starts a frame later, field.rs trigger_self). Now at 4,178 (0-55): the bandits' on-defeat bytecode 0x08073EFC (run_script(506) -> prop_show, then chance(25)) resumes 2 frames after prop_show in the game, 1 in ours; prop_show from every unit script is not it (breaks the take-off at 1,469)
* `r004_022_9-1` (004_d10_map): 1,580 / 2,671, **camera** - Some((1605, "cam: (80.8525390625, 0) vs game (80.6982421875, 0)"))
* `r009_006_4-1` (009_d16_desert): 396 / 9,485, **camera** - Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)"))
* `r012_014_12-19` (012_db5_a01): 1,493 / 3,122, **position / facing** - Some((1493, "pos: #27 sprite 188: (294.08984375, 121.76171875) vs game (294.38671875, 122.05859375)"))
* `r012_020_12-27` (012_db5_a01): 1,856 / 3,411, **RNG** - Some((1856, "rng: [942926ee, 25aeea64] vs game [de77fbf0, 756a9e8f]"))
* `r017_009_13-45` (017_db6_fukurou): 270 / 519, **RNG** - Some((270, "rng: [241c8739, 76b433d9] vs game [1824eac4, 5b21630a]"))
* `r017_022_13-47` (017_db6_fukurou): 225 / 428, **position / facing** - Some((225, "pos: #4 sprite 238: (351.90625, 207.46875) vs game (347.90625, 203.46875)"))
* `r033_002_6-1` (033_x05_s026): 9,904 / 36,816, **position / facing** - Some((9904, "pos: #129 sprite 43: (25.0078125, 184.9453125) vs game (25.0390625, 184.80078125)")) **Known:** Goku vs Majin Buu, 0-21 (was an RNG row at 9,545: the hit's screen shake now ticks through the pause fade-out). Now at 9,904: one punch's draws come in a different order - game: crit chance, rand(26), two chances (0x080192AB: the hit tree), the punch sfx / anim rands (0x080116DD), then the knockback rands (0x08018B4B: KnockbackSlide enter); ours: one hit-tree chance and the knockback before the punch rands - same count, different values (knock (-152, 44) vs (-144, 7))

### janemba

* `r036_002_1-5` (036_j01_hfil): 2,595 / 3,400, **RNG** - Some((2595, "rng: [c8663cf4, d2722128] vs game [1943d84d, 80803918]"))
* `r037_084_18-1` (037_j02_goku90): 465 / 687, **RNG** - Some((465, "rng: [06df649d, 58f5c11f] vs game [515ee011, 9cbe0118]"))
* `r038_001_18-1` (038_j03_goku120): 589 / 787, **camera** - Some((589, "cam: (53.080078125, 77.986328125) vs game (53.076171875, 77.986328125)"))
* `r038_002_18-2` (038_j03_goku120): 1,542 / 1,717, **menu timing** - Some((1563, "fade: black false vs game true")) **Known:** a heal menu closed in 18-2: predicted 21 black frames 208 cycles before the 22nd
* `r038_045_18-1` (038_j03_goku120): 251 / 818, **object set** - Some((251, "missing: obj #1 sprite 151 has no entity"))
* `r040_021_18-10` (040_h_maze): 568 / 7,545, **position / facing** - Some((568, "pos: #95 sprite 229: (225.5, 244) vs game (225, 244)"))
* `r042_001_18-11` (042_h_statues): 9,672 / 9,928, **position / facing** - Some((9672, "pos: #5 sprite 151: (144, 208) vs game (177, 132)"))
* `r044_015_18-1` (044_h_back1): 577 / 1,258, **menu timing** - Some((577, "fade: black true vs game false")) **Known:** Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by >= 21,578 cycles)
* `r046_045_18-2` (046_h_veg1): 754 / 1,682, **RNG** - Some((754, "rng: [4f3cec76, d4632e53] vs game [866b86b4, 2f27cc19]"))
* `r049_001_18-14` (049_j_sj2): 7,628 / 9,123, **sound** - Some((7628, "snd: [] vs game [\"play 41 32\"]"))

### superbuu

* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084, **camera** - Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))
* `r052_006_6-9` (052_m_gotenks): 4,688 / 24,489, **RNG** - Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]"))
* `r057_005_14-2` (057_g03_gohan141): 748 / 1,123, **position / facing** - Some((748, "pos: #4 sprite 256: (200, 200) vs game (200, 198)"))
* `r057_011_14-2` (057_g03_gohan141): 304 / 678, **RNG** - Some((304, "rng: [32c37f60, 7161f349] vs game [44eec898, d5acbac2]"))
* `r057_013_14-2` (057_g03_gohan141): 677 / 1,575, **position / facing** - Some((677, "pos: #4 sprite 256: (203.1875, 201.15234375) vs game (203.984375, 201.38671875)"))
* `r057_015_14-2` (057_g03_gohan141): 348 / 677, **position / facing** - Some((348, "pos: #3 sprite 256: (109.921875, 125) vs game (111.921875, 125)"))
* `r057_021_14-2` (057_g03_gohan141): 537 / 859, **position / facing** - Some((537, "pos: #4 sprite 256: (262.15625, 218.8515625) vs game (262.15625, 220.8515625)"))
* `r057_031_14-2` (057_g03_gohan141): 564 / 864, **RNG** - Some((564, "rng: [7d11f712, 74a63311] vs game [0b7b8092, a01ff008]"))
* `r057_035_14-2` (057_g03_gohan141): 626 / 914, **RNG** - Some((626, "rng: [e2c5877d, 6e8ce462] vs game [8b1da78c, f1845cae]"))
* `r057_059_14-2` (057_g03_gohan141): 650 / 1,800, **position / facing** - Some((650, "pos: #4 sprite 256: (180.078125, 200.984375) vs game (179.28125, 200.984375)"))
* `r057_066_14-1` (057_g03_gohan141): 3,503 / 7,327, **RNG** - Some((3503, "rng: [2203fea3, a2a3ad3b] vs game [140b8563, e41e1348]"))
* `r061_004_20-5` (061_g05_goku141): 5,251 / 16,442, **sound** - Some((5251, "snd: [] vs game [\"play 41 32\"]"))
* `r061_017_20-6` (061_g05_goku141): 897 / 1,183, **camera** - Some((911, "cam: (32, 80.9140625) vs game (32, 80)"))
* `r061_019_20-6` (061_g05_goku141): 841 / 1,135, **camera** - Some((855, "cam: (21, 80.796875) vs game (21, 80.7060546875)"))
* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177, **position / facing** - Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
* `r064_004_21-6` (064_m_kid_buu_saga): 254 / 1,041, **position / facing** - Some((254, "pos: #6 sprite 87: (112.26953125, 36.49609375) vs game (112.26953125, 35.99609375)"))
* `r064_029_21-26` (064_m_kid_buu_saga): 12,723 / 20,500, **camera** - Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415, **camera** - Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))
* `r067_136_14-1` (067_k03_goku158): 402 / 1,111, **menu timing** - Some((426, "fade: black false vs game true")) **Known:** Switch Character at 14-1 (Goku -> Vegeta): game 25 black frames, predicted 24 (46,986 short); the same save point's Trunks <-> Goten switches (r017_037) predict right with margins 86 k / 50 k
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
* `r069_168_14-1` (069_k05_vegeta150): 413 / 55,831, **menu timing** - Some((438, "fade: black false vs game true")) **Known:** Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore's black lasts 26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the 26th frame)
