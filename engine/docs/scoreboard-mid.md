# Parity scoreboard, every-N-frames cuts

Tree: `9a26dfd`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| earth | 1 | 52,415 | 46,426 (88.6%) | 47,841 (91.3%) | 4,574 | 0 | 52 | 26 | 0 |
| fat | 2 | 57,152 | 34,598 (60.5%) | 42,848 (75.0%) | 14,304 | 0 | 53 | 19 | 0 |
| superbuu | 5 | 80,492 | 22,641 (28.1%) | 40,156 (49.9%) | 39,336 | 1,000 | 72 | 27 | 1 |
| **all** | 8 | 190,059 | 103,665 (54.5%) | 130,845 (68.8%) | 58,214 | 1,000 | 177 | 72 | 1 |

## The whole game

Every frame of the real run: verified exact (an exact segment, a segment's matched prefix, or a tile), diverged
(after a first divergence, before the next cut), not covered (no importable cut before it: the run's start, a
refused tile). The opening's title / New Game / prologue before its first area entry count as verified by the
story playthrough test (same keys from power-on, every row compared, no import: 6,812 frames).

| chapter | run frames | verified exact | diverged | not covered |
|---|---|---|---|---|
| opening | 42,275 | 41,189 (97.4%) | 0 | 1,086 (2.6%) |
| earth | 160,503 | 152,322 (94.9%) | 5,243 | 2,938 (1.8%) |
| ship | 398,194 | 394,862 (99.2%) | 754 | 2,578 (0.6%) |
| fat | 388,646 | 365,098 (93.9%) | 15,108 | 8,440 (2.2%) |
| janemba | 199,703 | 186,412 (93.3%) | 879 | 12,412 (6.2%) |
| superbuu | 478,729 | 425,412 (88.9%) | 41,289 | 12,028 (2.5%) |
| **all** | 1,668,050 | 1,565,295 (93.8%) | 63,273 | 39,482 (2.4%) |

## Refused tiles by reason (for the importer)

| reason | tiles | frames | chapters |
|---|---|---|---|
| replay hung (> N s)")) | 1 | 1,000 | superbuu |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### earth

* `rE2__090_7-3` (E2_earth): 46,426 / 52,415 (Some((46426, "rng: [528efd5c, 5e494666] vs game [0b13ffab, f36ef4ea]"))) -> **47,841** / 4,574 / 0; 26 / 52 tiles exact
  * 2,211: sprite memory - Some((211, "vram: used blocks (start, size) ours only [], game only [(255, 40), (295, 14)]"))
  * 3,000: sprite memory - Some((0, "vram: used blocks (start, size) ours only [], game only [(25, 32), (57, 64), (185, 64), (249, 64)]"))
  * 4,005: sprite memory - Some((5, "vram: used blocks (start, size) ours only [(56, 1)], game only []"))
  * 5,000: sprite memory - Some((0, "vram: used blocks (start, size) ours only [], game only [(117, 32), (149, 64), (213, 64), (277, 64)]"))
  * 6,000: camera - Some((0, "cam: (243.19140625, 332.447265625) vs game (243.1923828125, 332.44921875)"))
  * 8,005: RNG - Some((5, "rng: [3bb9ca68, 742af456] vs game [cfca0a3c, e92c0cc5]"))
  * 10,029: sprite memory - Some((29, "vram: used blocks (start, size) ours only [], game only [(333, 8)]"))
  * 12,000: position / facing - Some((0, "hp: 152 vs game 157"))
  * 14,642: position / facing - Some((642, "pos: #14 sprite 261: (355.28125, 302.7109375) vs game (355.171875, 302.11328125)"))
  * 19,007: sprite memory - Some((7, "vram: used blocks (start, size) ours only [(285, 4)], game only []"))
  * 21,932: RNG - Some((932, "rng: [866fa560, d2512e0c] vs game [ed1832e0, 24362f57]"))
  * 22,045: screen fade - Some((45, "fade: black false vs game true"))
  * ... 14 more

### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336 (Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) -> **13,020** / 7,316 / 0; 5 / 20 tiles exact
  * 2,000: position / facing - Some((0, "pos: #9 sprite 269: (363, 176) vs game (363.109375, 173.58203125)"))
  * 4,029: sprite memory - Some((29, "vram: used blocks (start, size) ours only [], game only [(275, 4)]"))
  * 5,412: sprite memory - Some((412, "vram: free / spare lists differ (same used blocks): [-1] ours 13 game 12, [12] ours [255,14,13] game [255,13,255], [13
  * 6,000: sprite memory - Some((0, "vram: used blocks (start, size) ours only [], game only [(372, 16)]"))
  * 7,121: sprite memory - Some((121, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 9,000: position / facing - Some((0, "pos: #95 sprite 253: (407.90234375, 1000.00390625) vs game (402.7265625, 1000.2109375)"))
  * 10,302: sprite memory - Some((302, "vram: used blocks (start, size) ours only [], game only [(147, 4)]"))
  * 11,057: RNG - Some((57, "rng: [97049f9f, e415c6f4] vs game [db4919fa, 07b68a39]"))
  * 12,261: sprite memory - Some((261, "vram: used blocks (start, size) ours only [], game only [(147, 4)]"))
  * 13,000: position / facing - Some((0, "dir: #9 sprite 269: 0 vs game 1"))
  * 14,034: RNG - Some((34, "rng: [9bd4db0d, 3384f442] vs game [9109ef92, e203998d]"))
  * 17,000: position / facing - Some((0, "pos: #95 sprite 253: (407.90234375, 1000.00390625) vs game (402.7265625, 1000.2109375)"))
  * ... 3 more
* `r033_002_6-1` (033_x05_s026): 24,002 / 36,816 (Some((24002, "anim: #343 sprite 126: raw 16 cel 0 vs game raw 16 cel 1 tick 8"))) -> **29,828** / 6,988 / 0; 14 / 33 tiles exact
  * 1,100: sprite memory - Some((100, "vram: free / spare lists differ (same used blocks): [4] ours [1,427,1,3,5,0,255] game [1,427,1,3,5,255,255]"))
  * 3,994: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(185, 32), (217, 64), (281, 64), (345, 64)], game only []"))
  * 4,000: sprite memory - Some((2, "vram: used blocks (start, size) ours only [], game only [(185, 32), (217, 64), (281, 64), (345, 64)]"))
  * 5,000: sprite memory - Some((5, "vram: used blocks (start, size) ours only [], game only [(185, 32), (217, 64), (281, 64), (345, 64)]"))
  * 7,000: RNG - Some((0, "rng: [50f8e38b, 582be365] vs game [c2c5b6aa, a8e2e3bf]"))
  * 8,008: sprite memory - Some((8, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 9,824: sprite memory - Some((856, "vram: used blocks (start, size) ours only [(373, 4), (377, 4)], game only []"))
  * 10,009: sprite memory - Some((9, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 11,013: sprite memory - Some((13, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 12,014: sprite memory - Some((14, "vram: used blocks (start, size) ours only [(377, 4)], game only []"))
  * 15,152: script / text - Some((152, "text: game opens a text box: true, ours: false"))
  * 17,000: animation - Some((0, "anim: #21 sprite 123: raw 22 cel 8 tick 69 vs game raw 22 cel 10 tick 68"))
  * ... 7 more

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177 (Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30) -> **5,593** / 20,584 / 0; 3 / 25 tiles exact
  * 1,139: animation - Some((2, "anim: #2 sprite 280: raw 1 cel 0 tick 254 vs game raw 1 cel 1 tick 255"))
  * 2,176: position / facing - Some((176, "pos: #5 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
  * 3,000: position / facing - Some((0, "pos: #0 sprite 52: (116.515625, 212) vs game (116.3828125, 210)"))
  * 4,014: sound - Some((14, "snd: [] vs game [\"play 3 64\"]"))
  * 5,021: sound - Some((21, "snd: [] vs game [\"play 34 64\", \"stop 3\"]"))
  * 6,036: sound - Some((36, "snd: [] vs game [\"play 3 64\"]"))
  * 7,021: position / facing - Some((21, "dir: #0 sprite 52: 2 vs game 0"))
  * 8,165: script / text - Some((165, "text: game opens a text box: true, ours: false"))
  * 9,002: sprite memory - Some((2, "vram: used blocks (start, size) ours only [(257, 64)], game only []"))
  * 10,000: animation - Some((0, "anim: #1 sprite 115: raw 20 cel 0 vs game raw 20 cel 1 tick 193"))
  * 11,006: sprite memory - Some((6, "vram: used blocks (start, size) ours only [], game only [(377, 8)]"))
  * 12,000: position / facing - Some((0, "pos: #0 sprite 52: (321.421875, 200.515625) vs game (322.0859375, 198.3828125)"))
  * ... 10 more
* `r052_006_6-9` (052_m_gotenks): 4,688 / 24,489 (Some((4688, "rng: [ee2b9523, 199f850f] vs game [e968d49b, 3a47d41c]"))) -> **15,311** / 8,178 / 1,000; 9 / 22 tiles exact
  * 3,019: position / facing - Some((19, "dir: #2 sprite 123: 3 vs game 2"))
  * 4,000: camera - Some((0, "cam: (77, 12.8046875) vs game (76.5859375, 15.01953125)"))
  * 5,540: position / facing - Some((540, "pos: #4 sprite 49: (336.7109375, 309.10546875) vs game (335.51171875, 309.03125)"))
  * 6,000: refused - Some((0, "import: replay hung (> 600 s)"))
  * 9,773: sprite memory - Some((773, "vram: used blocks (start, size) ours only [(468, 8)], game only []"))
  * 10,996: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(187, 32), (219, 64), (283, 64), (363, 64)], game only []"))
  * 11,000: sprite memory - Some((4, "vram: used blocks (start, size) ours only [], game only [(187, 32), (219, 64), (283, 64), (363, 64)]"))
  * 13,012: sprite memory - Some((12, "vram: used blocks (start, size) ours only [(383, 4)], game only []"))
  * 15,337: RNG - Some((337, "rng: [9f5a0d30, eb8b0d2e] vs game [522ac42b, 94183eb3]"))
  * 16,170: position / facing - Some((170, "hp: 877 vs game 575"))
  * 17,000: screen fade - Some((0, "fade: black true vs game false"))
  * 19,005: object set - Some((5, "missing: obj #5 sprite 110 has no entity (ours there: sprite Some(108))"))
  * ... 1 more
* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084 (Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))) -> **13,973** / 6,111 / 0; 11 / 16 tiles exact
  * 2,498: sprite memory - Some((512, "vram: used blocks (start, size) ours only [(401, 32), (433, 4)], game only [(405, 32), (471, 4)]"))
  * 6,000: camera - Some((0, "cam: (79.7705078125, 361.5078125) vs game (79.76953125, 361.5078125)"))
  * 11,557: camera - Some((575, "cam: (0, 44) vs game (0.375, 43.71875)"))
  * 18,000: screen fade - Some((0, "fade: black true vs game false"))
  * 19,370: script / text - Some((370, "text: game opens a text box: true, ours: false"))
* `r057_066_14-1` (057_g03_gohan141): 3,527 / 7,327 (Some((3527, "pos: #243 sprite 49: (270.41015625, 164.78125) vs game (271.91015625, 164.863) -> **3,970** / 3,357 / 0; 3 / 7 tiles exact
  * 3,141: sprite memory - Some((141, "vram: used blocks (start, size) ours only [(127, 64), (191, 64), (255, 64), (379, 32)], game only []"))
  * 4,000: position / facing - Some((0, "pos: #6 sprite 123: (286.37890625, 113.41015625) vs game (286.33984375, 113.35546875)"))
  * 5,090: script / text - Some((90, "text: game opens a text box: true, ours: false"))
  * 6,026: screen fade - Some((26, "fade: black false vs game true"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415 (Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))) -> **1,309** / 1,106 / 0; 1 / 2 tiles exact
  * 1,374: sprite memory - Some((374, "vram: used blocks (start, size) ours only [(499, 4), (503, 1)], game only []"))
