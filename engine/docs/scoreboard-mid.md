# Parity scoreboard, every-N-frames cuts

Tree: `0dab418`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| fat | 2 | 25,385 | 12,214 (48.1%) | 20,336 (80.1%) | 5,049 | 0 | 24 | 14 | 0 |
| superbuu | 2 | 28,592 | 2,869 (10.0%) | 10,621 (37.1%) | 17,971 | 0 | 27 | 6 | 0 |
| **all** | 4 | 53,977 | 15,083 (27.9%) | 30,957 (57.4%) | 23,020 | 0 | 51 | 20 | 0 |

## The whole game

Every frame of the real run: verified exact (an exact segment, a segment's matched prefix, or a tile), diverged
(after a first divergence, before the next cut), not covered (no importable cut before it: the run's start, a
refused tile). The opening's title / New Game / prologue before its first area entry count as verified by the
story playthrough test (same keys from power-on, every row compared, no import: 6,812 frames).

| chapter | run frames | verified exact | diverged | not covered |
|---|---|---|---|---|
| opening | 42,275 | 41,189 (97.4%) | 0 | 1,086 (2.6%) |
| earth | 160,503 | 157,565 (98.2%) | 0 | 2,938 (1.8%) |
| ship | 398,194 | 395,616 (99.4%) | 0 | 2,578 (0.6%) |
| fat | 388,646 | 374,602 (96.4%) | 5,604 | 8,440 (2.2%) |
| janemba | 199,703 | 186,412 (93.3%) | 879 | 12,412 (6.2%) |
| superbuu | 478,729 | 449,730 (93.9%) | 17,971 | 11,028 (2.3%) |
| **all** | 1,668,050 | 1,605,114 (96.2%) | 24,454 | 38,482 (2.3%) |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336 (Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) -> **17,718** / 2,618 / 0; 13 / 20 tiles exact
  * 6,008: RNG - Some((8, "rng: [0d8cf402, 260df3cf] vs game [089b453c, a09338fc]"))
  * 7,192: sprite memory - Some((192, "vram: free / spare lists differ (same used blocks): [3] ours [1,371,1,2,16,255,255] game [1,371,1,2,15,255,255], [12] 
  * 10,596: camera - Some((623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)"))
  * 17,816: sprite memory - Some((816, "vram: free / spare lists differ (same used blocks): [7] ours [1,351,4,9,15,8,255] game [1,154,1,12,13,8,255], [8] ours
  * 18,014: animation - Some((14, "anim: #9 sprite 269: raw 5 cel 0 vs game raw 1 cel 0 tick 62"))
  * 19,292: sprite memory - Some((292, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 20,026: camera - Some((0, "cam: (160.75, 96.30859375) vs game (160, 101.875)"))
* `r010_011_11-13` (010_db2_desert): 1,618 / 5,049 (Some((1640, "cam: (240, 34.45703125) vs game (240, 34.71875)"))) -> **2,618** / 2,431 / 0; 1 / 4 tiles exact
  * 1,116: animation - Some((16, "anim: #3 sprite 123: raw 1 cel 0 vs game raw 2 cel 0 tick 0"))
  * 2,000: camera - Some((1, "cam: (185.0546875, 25) vs game (185.0517578125, 25)"))
  * 4,000: camera - Some((0, "cam: (0, 21.1728515625) vs game (0, 28.78125)"))

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177 (Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30) -> **9,312** / 16,865 / 0; 5 / 25 tiles exact
  * 1,139: animation - Some((2, "anim: #2 sprite 280: raw 1 cel 0 tick 254 vs game raw 1 cel 1 tick 255"))
  * 2,176: position / facing - Some((176, "pos: #5 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
  * 3,002: sprite memory - Some((2, "vram: used blocks (start, size) ours only [], game only [(369, 8)]"))
  * 4,315: position / facing - Some((315, "pos: #2 sprite 280: (182.68359375, 212.33203125) vs game (183.13671875, 211.81640625)"))
  * 5,107: position / facing - Some((107, "pos: #2 sprite 280: (287.90625, 197.44921875) vs game (288.8046875, 196.9375)"))
  * 6,130: position / facing - Some((130, "pos: #2 sprite 280: (224.19921875, 320.34375) vs game (224.36328125, 320.99609375)"))
  * 7,026: RNG - Some((26, "rng: [f5d56a63, 0f12b499] vs game [3fb5b6f3, 20803777]"))
  * 8,165: script / text - Some((165, "text: game opens a text box: true, ours: false"))
  * 9,025: RNG - Some((25, "rng: [a2751507, ee424df4] vs game [e69709d9, 799d8819]"))
  * 10,085: position / facing - Some((85, "pos: #2 sprite 280: (181.08984375, 191.49609375) vs game (180.390625, 191.34765625)"))
  * 11,197: position / facing - Some((197, "pos: #2 sprite 280: (161.3203125, 333.640625) vs game (160.61328125, 334.0078125)"))
  * 12,034: RNG - Some((34, "rng: [501e3619, ba16b501] vs game [d5dcc46b, 232dd0a3]"))
  * ... 8 more
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415 (Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))) -> **1,309** / 1,106 / 0; 1 / 2 tiles exact
  * 1,374: sprite memory - Some((374, "vram: used blocks (start, size) ours only [(499, 4), (503, 1)], game only []"))
