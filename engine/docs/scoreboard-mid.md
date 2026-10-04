# Parity scoreboard, every-N-frames cuts

Tree: `dd201a8`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| earth | 1 | 52,415 | 46,426 (88.6%) | 47,841 (91.3%) | 3,574 | 1,000 | 52 | 25 | 1 |
| ship | 1 | 19,859 | 19,859 (100.0%) | 19,859 (100.0%) | 0 | 0 | 19 | 10 | 2 |
| fat | 6 | 75,841 | 41,039 (54.1%) | 48,975 (64.6%) | 25,119 | 1,747 | 72 | 16 | 5 |
| janemba | 1 | 9,123 | 7,628 (83.6%) | 7,628 (83.6%) | 1,495 | 0 | 8 | 1 | 0 |
| superbuu | 18 | 190,533 | 64,526 (33.9%) | 112,436 (59.0%) | 64,089 | 14,008 | 178 | 68 | 16 |
| **all** | 27 | 347,771 | 179,478 (51.6%) | 236,739 (68.1%) | 94,277 | 16,755 | 329 | 120 | 24 |

## The whole game

Every frame of the real run: verified exact (an exact segment, a segment's matched prefix, or a tile), diverged
(after a first divergence, before the next cut), not covered (no importable cut before it: the run's start, a
refused tile).

| chapter | run frames | verified exact | diverged | not covered |
|---|---|---|---|---|
| opening | 42,275 | 34,377 (81.3%) | 0 | 7,898 (18.7%) |
| earth | 160,503 | 152,153 (94.8%) | 4,412 | 3,938 (2.5%) |
| ship | 398,194 | 395,616 (99.4%) | 0 | 2,578 (0.6%) |
| fat | 388,646 | 352,037 (90.6%) | 26,422 | 10,187 (2.6%) |
| janemba | 199,703 | 183,067 (91.7%) | 4,224 | 12,412 (6.2%) |
| superbuu | 478,729 | 376,964 (78.7%) | 76,729 | 25,036 (5.2%) |
| **all** | 1,668,050 | 1,494,214 (89.6%) | 111,787 | 62,049 (3.7%) |

## Refused tiles by reason (for the importer)

| reason | tiles | frames | chapters |
|---|---|---|---|
| unsupported cut: area AreaId { world: N, area: N, variant: N }: the load's on-enter script is still blocked (n | 9 | 9,000 | fat, ship, superbuu |
| unsupported cut: event sN node N: the waiting native NxNa is not among the node's actions")) | 4 | 4,000 | earth, fat, ship, superbuu |
| unsupported cut: event sN node N: the waiting native NxNd is not among the node's actions")) | 1 | 1,000 | fat |
| replay hung (> N s)")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [NcabNdb, NeNaN] after N draws vs RAM [NdeN, NfNceN]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [eNebNfN, NbNbbaN] after N draws vs RAM [NfNbN, NaNddec]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [dcNfN, NcNee] after N draws vs RAM [eNddNcN, NbN]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [NcNfNaaa, eNdN] after N draws vs RAM [NcN, NcNabN]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [eNdN, NaNcNd] after N draws vs RAM [NbN, NcNaNbN]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [NbbNbN, NcNeN] after N draws vs RAM [cecbN, ccdNcNd]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [NeN, NaaafbcN] after N draws vs RAM [dNbNfN, NcdNbf]")) | 1 | 1,000 | superbuu |
| import mismatch: RNG [fffNdbf, NddNecN] after N draws vs RAM [NeNd, beNfeNf]")) | 1 | 1,000 | superbuu |
| unsupported cut: event sN node N: the waiting native NxN is not among the node's actions")) | 1 | 1,000 | superbuu |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### earth

* `rE2__090_7-3` (E2_earth): 46,426 / 52,415 (Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]"))) -> **47,841** / 3,574 / 1,000; 25 / 52 tiles exact
  * 2,211: sprite memory - Some((211, "vram: used blocks (start, size) ours only [], game only [(255, 40), (295, 14)]"))
  * 3,000: sprite memory - Some((0, "vram: used blocks (start, size) ours only [], game only [(25, 32), (57, 64), (185, 64), (249, 64)]"))
  * 4,005: sprite memory - Some((5, "vram: used blocks (start, size) ours only [(56, 1)], game only []"))
  * 5,000: sprite memory - Some((0, "vram: used blocks (start, size) ours only [], game only [(117, 32), (149, 64), (213, 64), (277, 64)]"))
  * 6,000: camera - Some((0, "cam: (243.19140625, 332.447265625) vs game (243.1923828125, 332.44921875)"))
  * 8,013: RNG - Some((13, "rng: [1de73f0b, 43d27031] vs game [63608b4d, 47a6d2d4]"))
  * 10,029: sprite memory - Some((29, "vram: used blocks (start, size) ours only [], game only [(333, 8)]"))
  * 12,000: position / facing - Some((0, "hp: 152 vs game 157"))
  * 14,642: position / facing - Some((642, "pos: #14 sprite 261: (355.28125, 302.7109375) vs game (355.171875, 302.11328125)"))
  * 19,007: sprite memory - Some((7, "vram: used blocks (start, size) ours only [(285, 4)], game only []"))
  * 21,932: RNG - Some((932, "rng: [866fa560, d2512e0c] vs game [ed1832e0, 24362f57]"))
  * 22,045: screen fade - Some((45, "fade: black false vs game true"))
  * ... 15 more

### ship

* `r000_002_7-2` (000_ship_c14): 19,859 / 19,859 (None) -> **19,859** / 0 / 0; 10 / 19 tiles exact
  * 1,009: animation - Some((9, "anim: #2 sprite 138: raw 13 cel 0 tick 16 vs game raw 13 cel 1 tick 16"))
  * 2,206: sprite memory - Some((230, "vram: used blocks (start, size) ours only [(409, 4)], game only []"))
  * 5,000: refused - Some((0, "import: unsupported cut: event s307 node 25: the waiting native 0x4a is not among the node's actions"))
  * 6,000: camera - Some((0, "cam: (196.439453125, 281.5771484375) vs game (196.5654296875, 281.892578125)"))
  * 7,583: RNG - Some((583, "rng: [514fca98, ee12e8e7] vs game [ba096fee, ccc370a0]"))
  * 8,068: animation - Some((68, "anim: #0 sprite 114: raw 21 cel 1 vs game raw 21 cel 0 tick 95"))
  * 9,000: animation - Some((1, "anim: #1 sprite 158: raw 1 cel 0 tick 2188 vs game raw 2 cel 0 tick 0"))
  * 17,031: sprite memory - Some((31, "vram: used blocks (start, size) ours only [], game only [(81, 32), (113, 64), (177, 64), (241, 64)]"))
  * 18,000: refused - Some((0, "import: unsupported cut: area AreaId { world: 8, area: 1, variant: 1 }: the load's on-enter script is still blocked (no 

### fat

* `r009_006_4-1` (009_d16_desert): 396 / 9,485 (Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)"))) -> **881** / 8,604 / 0; 1 / 9 tiles exact
  * 1,000: position / facing - Some((0, "pos: #5 sprite 261: (322.5, 360) vs game (322, 360)"))
  * 2,000: position / facing - Some((0, "pos: #11 sprite 69: (736, 386) vs game (736, 386.5)"))
  * 3,000: position / facing - Some((0, "pos: #6 sprite 180: (118.5, 344) vs game (118, 344)"))
  * 4,000: position / facing - Some((0, "pos: #3 sprite 36: (680, 547.5) vs game (680, 548)"))
  * 5,000: position / facing - Some((0, "pos: #3 sprite 36: (648, 530) vs game (648, 530.5)"))
  * 6,000: position / facing - Some((0, "pos: #3 sprite 36: (632, 549) vs game (632, 548.5)"))
  * 7,000: position / facing - Some((0, "pos: #3 sprite 36: (648, 528) vs game (648.5, 528)"))
  * 8,000: position / facing - Some((0, "pos: #3 sprite 36: (615.5, 544) vs game (616, 544)"))
* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336 (Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) -> **12,143** / 7,857 / 336; 2 / 20 tiles exact
  * 1,000: position / facing - Some((0, "pos: #3 sprite 36: (633, 512) vs game (632.5, 512)"))
  * 2,000: position / facing - Some((0, "pos: #9 sprite 269: (363, 176) vs game (363.109375, 173.58203125)"))
  * 3,575: position / facing - Some((575, "pos: #100 sprite 10: (607.75, 468) vs game (607.65625, 467.5)"))
  * 4,005: sprite memory - Some((5, "vram: used blocks (start, size) ours only [(263, 16)], game only []"))
  * 5,133: position / facing - Some((133, "pos: #100 sprite 10: (2284, 1046.171875) vs game (2284.5, 1045.88671875)"))
  * 6,000: sprite memory - Some((0, "vram: used blocks (start, size) ours only [], game only [(372, 16)]"))
  * 7,121: sprite memory - Some((121, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 9,000: position / facing - Some((0, "pos: #95 sprite 253: (407.90234375, 1000.00390625) vs game (402.7265625, 1000.2109375)"))
  * 10,052: position / facing - Some((52, "pos: #101 sprite 10: (632, 1827.5) vs game (632, 1828)"))
  * 11,010: sprite memory - Some((10, "vram: used blocks (start, size) ours only [(279, 16)], game only []"))
  * 12,012: position / facing - Some((12, "pos: #100 sprite 10: (607.75, 468) vs game (607.65625, 467.5)"))
  * 13,000: position / facing - Some((0, "dir: #9 sprite 269: 0 vs game 1"))
  * ... 6 more
* `r033_002_6-1` (033_x05_s026): 24,002 / 36,816 (Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8"))) -> **29,906** / 6,910 / 0; 12 / 36 tiles exact
  * 1,000: position / facing - Some((0, "pos: #2 sprite 117: (320, 0) vs game (318.72265625, 1.51953125)"))
  * 3,994: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(185, 32), (217, 64), (281, 64), (345, 64)], game only []"))
  * 4,000: sprite memory - Some((2, "vram: used blocks (start, size) ours only [], game only [(185, 32), (217, 64), (281, 64), (345, 64)]"))
  * 5,000: sprite memory - Some((5, "vram: used blocks (start, size) ours only [], game only [(185, 32), (217, 64), (281, 64), (345, 64)]"))
  * 6,339: position / facing - Some((339, "pos: #0 sprite 43: (135.203125, 160.2109375) vs game (135.203125, 160.21875)"))
  * 7,000: RNG - Some((0, "rng: [50f8e38b, 582be365] vs game [c2c5b6aa, a8e2e3bf]"))
  * 8,008: sprite memory - Some((8, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 9,020: position / facing - Some((20, "pos: #0 sprite 43: (26.8125, 181.80859375) vs game (26.8125, 181.77734375)"))
  * 10,009: sprite memory - Some((9, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 11,013: sprite memory - Some((13, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 12,014: sprite memory - Some((14, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 15,152: script / text - Some((152, "text: game opens a text box: true, ours: false"))
  * ... 12 more
* `r012_014_12-19` (012_db5_a01): 1,518 / 3,122 (Some((1518, "rng: [2150ae29, 1e6e620f] vs game [c3a6143a, e7217e82]"))) -> **1,518** / 604 / 1,000; 0 / 2 tiles exact
  * 1,000: position / facing - Some((0, "pos: #0 sprite 188: (178.58203125, 126.71875) vs game (179.02734375, 127.1640625)"))
  * 2,000: refused - Some((0, "import: unsupported cut: area AreaId { world: 12, area: 19, variant: 1 }: the load's on-enter script is still blocked (n
* `r012_020_12-27` (012_db5_a01): 1,856 / 3,411 (Some((1856, "rng: [942926ee, 25aeea64] vs game [de77fbf0, 756a9e8f]"))) -> **1,856** / 1,144 / 411; 0 / 3 tiles exact
  * 1,044: RNG - Some((44, "rng: [35958795, e1b03d77] vs game [16f88f5d, 4f982150]"))
  * 2,000: position / facing - Some((0, "dir: #2 sprite 31: 2 vs game 0"))
  * 3,000: refused - Some((0, "import: unsupported cut: area AreaId { world: 12, area: 27, variant: 1 }: the load's on-enter script is still blocked (n
* `r004_022_9-1` (004_d10_map): 2,671 / 2,671 (None) -> **2,671** / 0 / 0; 1 / 2 tiles exact
  * 2,000: refused - Some((0, "import: unsupported cut: event s011 node 3: the waiting native 0x8d is not among the node's actions"))

### janemba

* `r049_001_18-14` (049_j_sj2): 7,628 / 9,123 (Some((7628, "snd: [] vs game [\"play 41 32\"]"))) -> **7,628** / 1,495 / 0; 1 / 8 tiles exact
  * 1,005: position / facing - Some((5, "pos: #2 sprite 147: (116, 68) vs game (116, 67)"))
  * 2,006: position / facing - Some((6, "pos: #2 sprite 147: (116, 67) vs game (116, 68)"))
  * 3,006: position / facing - Some((6, "pos: #2 sprite 147: (116, 67) vs game (116, 68)"))
  * 4,003: RNG - Some((3, "rng: [cafd6caa, 9823db58] vs game [55eaf243, 92bb4090]"))
  * 6,036: sprite memory - Some((36, "vram: used blocks (start, size) ours only [(446, 1)], game only []"))
  * 7,595: animation - Some((595, "anim: #0 sprite 102: raw 1 cel 0 vs game raw 2 cel 0 tick 7"))
  * 8,000: animation - Some((0, "anim: #1 sprite 89: raw 1 cel 2 tick 17 vs game raw 1 cel 3 tick 17"))

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177 (Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30) -> **5,227** / 12,950 / 8,000; 3 / 25 tiles exact
  * 1,000: screen fade - Some((0, "fade: black true vs game false"))
  * 2,176: position / facing - Some((176, "pos: #5 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
  * 3,000: refused - Some((0, "import: import mismatch: RNG [58cab0db, 6e9a6325] after 17 draws vs RAM [79de3589, 0f5ce720]"))
  * 4,000: position / facing - Some((0, "pos: #0 sprite 52: (279.453125, 257.09375) vs game (282.51953125, 256.9609375)"))
  * 5,021: sound - Some((21, "snd: [] vs game [\"play 34 64\", \"stop 3\"]"))
  * 6,000: position / facing - Some((0, "pos: #0 sprite 52: (116.015625, 204) vs game (115.8828125, 202)"))
  * 7,000: position / facing - Some((0, "pos: #0 sprite 52: (310, 121.1328125) vs game (312, 121.53125)"))
  * 8,000: refused - Some((0, "import: import mismatch: RNG [e6eb3f68, 3b74bba7] after 26 draws vs RAM [36f59b02, 18a6ddec]"))
  * 9,002: sprite memory - Some((2, "vram: used blocks (start, size) ours only [(257, 64)], game only []"))
  * 10,000: refused - Some((0, "import: import mismatch: RNG [dc932f42, 98c848ee] after 15 draws vs RAM [e0dd58c3, 9b239774]"))
  * 11,000: position / facing - Some((0, "pos: #0 sprite 52: (326.0703125, 185.5859375) vs game (325.40625, 187.71875)"))
  * 12,000: refused - Some((0, "import: import mismatch: RNG [7c8f8aaa, e55d4669] after 21 draws vs RAM [6998c707, 3c7ab561]"))
  * ... 10 more
* `r069_168_14-1` (069_k05_vegeta150): 20,168 / 55,831 (Some((20168, "pos: #271 sprite 47: (268.046875, 210.015625) vs game (268.046875, 210.14843) -> **41,365** / 14,466 / 0; 30 / 55 tiles exact
  * 4,000: refused - Some((0, "import: unsupported cut: event s480 node 16: the waiting native 0x88 is not among the node's actions"))
  * 11,007: sprite memory - Some((7, "vram: used blocks (start, size) ours only [(363, 4)], game only []"))
  * 13,015: sprite memory - Some((15, "vram: used blocks (start, size) ours only [(365, 4)], game only []"))
  * 14,005: sprite memory - Some((5, "vram: used blocks (start, size) ours only [(349, 4)], game only []"))
  * 17,834: sprite memory - Some((834, "vram: free / spare lists differ (same used blocks): [19] ours [1,349,16,20,14,0,255] game [1,349,16,20,14,0,12]"))
  * 18,015: sound - Some((15, "snd: [] vs game [\"play 33 64\", \"stop 32\"]"))
  * 23,717: sprite memory - Some((717, "vram: used blocks (start, size) ours only [(365, 16)], game only []"))
  * 24,000: position / facing - Some((0, "pos: #7 sprite 47: (132074.46875, 132125.171875) vs game (230, 279.984375)"))
  * 27,994: object set - Some((999, "extra: entity 6 sprite Some(77) (npc) not in the game"))
  * 28,000: RNG - Some((20, "rng: [1c792c27, e4fbc64b] vs game [f26d4e8b, 3a11455b]"))
  * 30,000: animation - Some((0, "anim: #8 sprite 43: raw 19 cel 0 tick 0 vs game raw 19 cel 3 tick 51"))
  * 31,110: other - Some((133, "area: (0, 19, 1) vs game (19, 3, 1)"))
  * ... 13 more
* `r052_006_6-9` (052_m_gotenks): 4,688 / 24,489 (Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]"))) -> **13,958** / 7,531 / 3,000; 6 / 24 tiles exact
  * 1,994: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(51, 32), (83, 64), (147, 64), (211, 64)], game only []"))
  * 2,000: sprite memory - Some((2, "vram: used blocks (start, size) ours only [], game only [(51, 32), (83, 64), (147, 64), (211, 64)]"))
  * 3,019: position / facing - Some((19, "dir: #2 sprite 123: 3 vs game 2"))
  * 4,000: camera - Some((0, "cam: (77, 12.8046875) vs game (76.5859375, 15.01953125)"))
  * 5,540: position / facing - Some((540, "pos: #4 sprite 49: (336.7109375, 309.10546875) vs game (335.51171875, 309.03125)"))
  * 6,000: refused - Some((0, "import: replay hung (> 600 s)"))
  * 9,773: sprite memory - Some((773, "vram: used blocks (start, size) ours only [(468, 8)], game only []"))
  * 10,996: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(187, 32), (219, 64), (283, 64), (363, 64)], game only []"))
  * 11,000: sprite memory - Some((4, "vram: used blocks (start, size) ours only [], game only [(187, 32), (219, 64), (283, 64), (363, 64)]"))
  * 13,012: sprite memory - Some((12, "vram: used blocks (start, size) ours only [(383, 4)], game only []"))
  * 15,312: RNG - Some((312, "rng: [90fdd1f7, 965b007a] vs game [f20023f2, 0ba19f9d]"))
  * 16,001: position / facing - Some((1, "dir: #5 sprite 49: 3 vs game 1"))
  * ... 6 more
* `r064_029_21-26` (064_m_kid_buu_saga): 12,723 / 20,500 (Some((12784, "cam: (37.1884765625, 502.986328125) vs game (37.208984375, 502.9853515625)")) -> **13,223** / 7,277 / 0; 4 / 20 tiles exact
  * 2,202: object set - Some((202, "extra: entity 6 sprite Some(280) (npc) not in the game"))
  * 3,010: sprite memory - Some((10, "vram: used blocks (start, size) ours only [(319, 4)], game only []"))
  * 4,086: RNG - Some((86, "rng: [20b411b6, a1a0344b] vs game [ebd21868, 0381af43]"))
  * 6,000: animation - Some((0, "anim: #0 sprite 49: raw 13 cel 2 tick 17 vs game raw 13 cel 3 tick 17"))
  * 7,118: object set - Some((118, "missing: obj #1 sprite 138 has no entity"))
  * 8,000: RNG - Some((7, "rng: [8bce58cf, 7d1f1bf7] vs game [eafe7255, 0156d1a2]"))
  * 10,055: position / facing - Some((55, "pos: #3 sprite 45: (295, 200) vs game (296, 200)"))
  * 11,000: animation - Some((0, "anim: #0 sprite 47: raw 13 cel 0 tick 0 vs game raw 13 cel 5 tick 80"))
  * 12,000: position / facing - Some((0, "pos: #94 sprite 253: (365.03125, 1001.71875) vs game (365.12890625, 1001.71484375)"))
  * 13,000: position / facing - Some((0, "pos: #4 sprite 181: (848, 256) vs game (848, 255.5)"))
  * 14,000: position / facing - Some((0, "pos: #3 sprite 36: (629, 544) vs game (628.5, 544)"))
  * 15,000: position / facing - Some((0, "pos: #4 sprite 181: (756, 248) vs game (755.5, 248)"))
  * ... 4 more
* `r057_066_14-1` (057_g03_gohan141): 3,527 / 7,327 (Some((3527, "pos: #243 sprite 49: (270.41015625, 164.78125) vs game (271.91015625, 164.863) -> **3,970** / 2,357 / 1,000; 3 / 7 tiles exact
  * 3,141: sprite memory - Some((141, "vram: used blocks (start, size) ours only [(127, 64), (191, 64), (255, 64), (379, 32)], game only []"))
  * 4,000: refused - Some((0, "import: unsupported cut: event s235 node 0: the waiting native 0x3a is not among the node's actions"))
  * 5,090: script / text - Some((90, "text: game opens a text box: true, ours: false"))
  * 6,026: screen fade - Some((26, "fade: black false vs game true"))
* `r061_004_20-5` (061_g05_goku141): 5,251 / 16,442 (Some((5251, "snd: [] vs game [\"play 41 32\"]"))) -> **13,246** / 3,196 / 0; 9 / 16 tiles exact
  * 3,000: object set - Some((0, "missing: obj #2 sprite 77 has no entity"))
  * 4,000: position / facing - Some((0, "pos: #6 sprite 45: (248, 212) vs game (247, 212)"))
  * 5,251: sound - Some((251, "snd: [] vs game [\"play 41 32\"]"))
  * 6,008: sprite memory - Some((8, "vram: used blocks (start, size) ours only [(451, 4)], game only []"))
  * 13,987: sprite memory - Some((995, "vram: used blocks (start, size) ours only [(401, 16), (417, 2), (419, 16), (435, 60)], game only [(461, 16), (477, 2),
  * 14,000: camera - Some((0, "cam: (0, 0) vs game (0, 40)"))
  * 16,000: camera - Some((0, "cam: (232, 49.59375) vs game (120, 128)"))
* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084 (Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))) -> **16,927** / 3,157 / 0; 13 / 19 tiles exact
  * 2,498: sprite memory - Some((512, "vram: used blocks (start, size) ours only [(401, 32), (433, 4)], game only [(405, 32), (471, 4)]"))
  * 6,000: camera - Some((0, "cam: (79.7705078125, 361.5078125) vs game (79.76953125, 361.5078125)"))
  * 11,557: camera - Some((575, "cam: (0, 44) vs game (0.375, 43.71875)"))
  * 17,000: screen fade - Some((0, "fade: black true vs game false"))
  * 18,000: position / facing - Some((0, "pos: #2 sprite 123: (192, 88) vs game (185, 0)"))
  * 19,370: script / text - Some((370, "text: game opens a text box: true, ours: false"))
* `r069_161_14-2` (069_k05_vegeta150): 166 / 2,174 (Some((166, "pos: #3 sprite 256: (89.8203125, 130.578125) vs game (91.8203125, 130.578125)") -> **172** / 2,002 / 0; 0 / 1 tiles exact
  * 1,006: animation - Some((6, "anim: #3 sprite 280: raw 6 cel 0 vs game raw 5 cel 0 tick 0"))
* `r069_125_14-2` (069_k05_vegeta150): 87 / 2,012 (Some((87, "rng: [5f0fda5a, 018eaccf] vs game [60553a04, 72fd0483]"))) -> **87** / 1,925 / 0; 0 / 1 tiles exact
  * 1,000: camera - Some((0, "cam: (54.158203125, 119.0205078125) vs game (53.6259765625, 119.703125)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415 (Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))) -> **693** / 1,307 / 415; 0 / 2 tiles exact
  * 1,000: position / facing - Some((0, "pos: #1 sprite 255: (219.17578125, 195.43359375) vs game (219.17578125, 195.4296875)"))
  * 2,000: refused - Some((0, "import: unsupported cut: area AreaId { world: 14, area: 2, variant: 1 }: the load's on-enter script is still blocked (no
* `r069_021_14-2` (069_k05_vegeta150): 576 / 2,149 (Some((576, "pos: #3 sprite 256: (103.546875, 116.1015625) vs game (103.546875, 118.1015625) -> **576** / 1,573 / 0; 0 / 1 tiles exact
  * 1,000: animation - Some((0, "anim: #2 sprite 280: raw 5 cel 0 vs game raw 6 cel 0 tick 0"))
* `r069_077_14-2` (069_k05_vegeta150): 246 / 1,666 (Some((246, "rng: [25d9021b, ee0d7ff5] vs game [c6a9c179, b03716c7]"))) -> **253** / 1,413 / 0; 0 / 1 tiles exact
  * 1,007: animation - Some((7, "anim: #3 sprite 280: raw 6 cel 0 vs game raw 5 cel 0 tick 1"))
* `r057_059_14-2` (057_g03_gohan141): 650 / 1,800 (Some((650, "pos: #4 sprite 256: (180.078125, 200.984375) vs game (179.28125, 200.984375)")) -> **650** / 350 / 800; 0 / 1 tiles exact
  * 1,000: refused - Some((0, "import: unsupported cut: area AreaId { world: 14, area: 2, variant: 1 }: the load's on-enter script is still blocked (no
* `r069_089_14-2` (069_k05_vegeta150): 331 / 1,462 (Some((331, "rng: [4e913927, 916a3cd0] vs game [c036c948, 51b277db]"))) -> **339** / 1,123 / 0; 0 / 1 tiles exact
  * 1,008: sprite memory - Some((8, "vram: used blocks (start, size) ours only [(397, 4)], game only []"))
* `r069_003_14-2` (069_k05_vegeta150): 413 / 1,554 (Some((413, "pos: #3 sprite 256: (109.453125, 135.74609375) vs game (109.453125, 137.746093) -> **440** / 1,114 / 0; 0 / 1 tiles exact
  * 1,027: position / facing - Some((27, "pos: #1 sprite 256: (333.125, 205.38671875) vs game (333.125, 205.4296875)"))
* `r069_101_14-2` (069_k05_vegeta150): 311 / 1,406 (Some((311, "pos: #3 sprite 256: (79.37890625, 105.875) vs game (77.37890625, 105.875)"))) -> **344** / 1,062 / 0; 0 / 1 tiles exact
  * 1,033: animation - Some((33, "anim: #2 sprite 280: raw 6 cel 0 vs game raw 5 cel 0 tick 0"))
* `r069_117_14-2` (069_k05_vegeta150): 738 / 1,793 (Some((738, "pos: #4 sprite 256: (247.90625, 198.87109375) vs game (248.703125, 198.8945312) -> **738** / 262 / 793; 0 / 1 tiles exact
  * 1,000: refused - Some((0, "import: unsupported cut: area AreaId { world: 14, area: 2, variant: 1 }: the load's on-enter script is still blocked (no
* `r069_139_14-2` (069_k05_vegeta150): 225 / 1,252 (Some((225, "pos: #3 sprite 256: (88.24609375, 142.53125) vs game (88.24609375, 143.328125)) -> **228** / 1,024 / 0; 0 / 1 tiles exact
  * 1,003: position / facing - Some((3, "pos: #1 sprite 256: (268.796875, 179.77734375) vs game (269.296875, 179.63671875)"))
