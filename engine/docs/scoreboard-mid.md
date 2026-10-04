# Parity scoreboard, every-N-frames cuts

Tree: `fda75a6`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| fat | 1 | 20,336 | 10,596 (52.1%) | 16,846 (82.8%) | 3,490 | 0 | 20 | 10 | 0 |
| superbuu | 3 | 48,676 | 14,426 (29.6%) | 23,902 (49.1%) | 24,774 | 0 | 43 | 18 | 0 |
| **all** | 4 | 69,012 | 25,022 (36.3%) | 40,748 (59.0%) | 28,264 | 0 | 63 | 28 | 0 |

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
| fat | 388,646 | 375,912 (96.7%) | 4,294 | 8,440 (2.2%) |
| janemba | 199,703 | 186,412 (93.3%) | 879 | 12,412 (6.2%) |
| superbuu | 478,729 | 442,927 (92.5%) | 24,774 | 11,028 (2.3%) |
| **all** | 1,668,050 | 1,599,621 (95.9%) | 29,947 | 38,482 (2.3%) |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336 (Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) -> **16,846** / 3,490 / 0; 10 / 20 tiles exact
  * 4,077: sprite memory - Some((77, "vram: free / spare lists differ (same used blocks): [-1] ours 4 game 12, [4] ours [255,12,255] game [255,13,12], [12] o
  * 5,412: sprite memory - Some((412, "vram: free / spare lists differ (same used blocks): [-1] ours 13 game 12, [12] ours [255,14,13] game [255,13,255], [13
  * 6,008: RNG - Some((8, "rng: [d7559509, b8a479f4] vs game [089b453c, a09338fc]"))
  * 7,192: sprite memory - Some((192, "vram: used blocks (start, size) ours only [(151, 4), (354, 16), (370, 1)], game only [(154, 1), (351, 16), (367, 4)]")
  * 10,596: camera - Some((623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)"))
  * 14,128: sprite memory - Some((128, "vram: free / spare lists differ (same used blocks): [-1] ours 16 game 15, [4] ours [255,17,15] game [255,16,15], [15] 
  * 17,816: sprite memory - Some((816, "vram: used blocks (start, size) ours only [(151, 4), (338, 16), (354, 1)], game only [(154, 1), (335, 16), (351, 4)]")
  * 18,014: animation - Some((14, "anim: #9 sprite 269: raw 5 cel 0 vs game raw 1 cel 0 tick 62"))
  * 19,292: sprite memory - Some((292, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 20,026: camera - Some((0, "cam: (160.75, 96.30859375) vs game (160, 101.875)"))

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177 (Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30) -> **7,990** / 18,187 / 0; 5 / 25 tiles exact
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
  * ... 8 more
* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084 (Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))) -> **14,603** / 5,481 / 0; 12 / 16 tiles exact
  * 2,498: sprite memory - Some((512, "vram: used blocks (start, size) ours only [(401, 32), (433, 4)], game only [(405, 32), (471, 4)]"))
  * 6,000: camera - Some((0, "cam: (79.7705078125, 361.5078125) vs game (79.76953125, 361.5078125)"))
  * 11,557: camera - Some((575, "cam: (0, 44) vs game (0.375, 43.71875)"))
  * 18,000: screen fade - Some((0, "fade: black true vs game false"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415 (Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))) -> **1,309** / 1,106 / 0; 1 / 2 tiles exact
  * 1,374: sprite memory - Some((374, "vram: used blocks (start, size) ours only [(499, 4), (503, 1)], game only []"))
