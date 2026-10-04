# Parity scoreboard, every-N-frames cuts

Tree: `fe7ec4a`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| fat | 2 | 57,152 | 41,155 (72.0%) | 50,016 (87.5%) | 7,136 | 0 | 53 | 28 | 0 |
| superbuu | 4 | 73,165 | 29,763 (40.7%) | 42,997 (58.8%) | 30,168 | 0 | 65 | 27 | 0 |
| **all** | 6 | 130,317 | 70,918 (54.4%) | 93,013 (71.4%) | 37,304 | 0 | 118 | 55 | 0 |

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
| fat | 388,646 | 372,266 (95.8%) | 7,940 | 8,440 (2.2%) |
| janemba | 199,703 | 186,412 (93.3%) | 879 | 12,412 (6.2%) |
| superbuu | 478,729 | 437,533 (91.4%) | 30,168 | 11,028 (2.3%) |
| **all** | 1,668,050 | 1,590,581 (95.4%) | 38,987 | 38,482 (2.3%) |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### fat

* `r002_006_4-1` (002_d04_bandana): 10,596 / 20,336 (Some((10623, "cam: (178.77734375, 237.79296875) vs game (178.9638671875, 237.7021484375)")) -> **15,641** / 4,695 / 0; 8 / 20 tiles exact
  * 2,079: sprite memory - Some((94, "vram: used blocks (start, size) ours only [(12, 16), (28, 16), (44, 16), (60, 16), (76, 16), (92, 16), (108, 197), (305
  * 4,029: sprite memory - Some((29, "vram: used blocks (start, size) ours only [], game only [(275, 4)]"))
  * 5,412: sprite memory - Some((412, "vram: free / spare lists differ (same used blocks): [-1] ours 13 game 12, [12] ours [255,14,13] game [255,13,255], [13
  * 6,008: RNG - Some((8, "rng: [d7559509, b8a479f4] vs game [089b453c, a09338fc]"))
  * 7,121: sprite memory - Some((121, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 10,302: sprite memory - Some((302, "vram: used blocks (start, size) ours only [], game only [(147, 4)]"))
  * 13,066: sprite memory - Some((81, "vram: used blocks (start, size) ours only [(158, 16), (174, 197), (371, 1), (383, 1), (384, 16), (400, 16), (416, 16), 
  * 14,085: sprite memory - Some((85, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 17,745: sprite memory - Some((745, "vram: used blocks (start, size) ours only [], game only [(151, 4)]"))
  * 18,014: animation - Some((14, "anim: #9 sprite 269: raw 5 cel 0 vs game raw 1 cel 0 tick 62"))
  * 19,135: sprite memory - Some((135, "vram: used blocks (start, size) ours only [], game only [(275, 4)]"))
  * 20,026: camera - Some((0, "cam: (160.75, 96.30859375) vs game (160, 101.875)"))
* `r033_002_6-1` (033_x05_s026): 30,559 / 36,816 (Some((30559, "snd: [] vs game [\"song 6\"]"))) -> **34,375** / 2,441 / 0; 20 / 33 tiles exact
  * 1,100: sprite memory - Some((100, "vram: free / spare lists differ (same used blocks): [4] ours [1,427,1,3,5,0,255] game [1,427,1,3,5,255,255]"))
  * 3,994: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(185, 32), (217, 64), (281, 64), (345, 64)], game only []"))
  * 4,000: sprite memory - Some((2, "vram: used blocks (start, size) ours only [], game only [(185, 32), (217, 64), (281, 64), (345, 64)]"))
  * 5,000: sprite memory - Some((5, "vram: used blocks (start, size) ours only [], game only [(185, 32), (217, 64), (281, 64), (345, 64)]"))
  * 7,000: RNG - Some((0, "rng: [50f8e38b, 582be365] vs game [c2c5b6aa, a8e2e3bf]"))
  * 15,152: script / text - Some((152, "text: game opens a text box: true, ours: false"))
  * 17,000: animation - Some((0, "anim: #21 sprite 123: raw 22 cel 8 tick 69 vs game raw 22 cel 10 tick 68"))
  * 18,001: sprite memory - Some((1, "vram: used blocks (start, size) ours only [(316, 1)], game only []"))
  * 19,119: sprite memory - Some((119, "vram: used blocks (start, size) ours only [(257, 16)], game only []"))
  * 22,000: position / facing - Some((0, "pos: #19 sprite 123: (147, 264) vs game (149.78125, 264)"))
  * 27,509: sprite memory - Some((509, "vram: used blocks (start, size) ours only [(375, 4)], game only []"))
  * 31,697: position / facing - Some((0, "dir: #2 sprite 138: 0 vs game 1"))
  * ... 1 more

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177 (Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30) -> **7,545** / 18,632 / 0; 4 / 25 tiles exact
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
  * ... 9 more
* `r052_004_1-5` (052_m_gotenks): 11,557 / 20,084 (Some((11575, "cam: (0, 44) vs game (0.375, 43.71875)"))) -> **14,603** / 5,481 / 0; 12 / 16 tiles exact
  * 2,498: sprite memory - Some((512, "vram: used blocks (start, size) ours only [(401, 32), (433, 4)], game only [(405, 32), (471, 4)]"))
  * 6,000: camera - Some((0, "cam: (79.7705078125, 361.5078125) vs game (79.76953125, 361.5078125)"))
  * 11,557: camera - Some((575, "cam: (0, 44) vs game (0.375, 43.71875)"))
  * 18,000: screen fade - Some((0, "fade: black true vs game false"))
* `r052_006_6-9` (052_m_gotenks): 15,337 / 24,489 (Some((15337, "rng: [9f5a0d30, eb8b0d2e] vs game [522ac42b, 94183eb3]"))) -> **19,540** / 4,949 / 0; 10 / 22 tiles exact
  * 3,019: position / facing - Some((19, "dir: #2 sprite 123: 3 vs game 2"))
  * 4,000: camera - Some((0, "cam: (77, 12.8046875) vs game (76.5859375, 15.01953125)"))
  * 6,017: position / facing - Some((17, "dir: #2 sprite 126: 0 vs game 1"))
  * 9,773: sprite memory - Some((773, "vram: used blocks (start, size) ours only [(468, 8)], game only []"))
  * 10,996: sprite memory - Some((999, "vram: used blocks (start, size) ours only [(187, 32), (219, 64), (283, 64), (363, 64)], game only []"))
  * 11,000: sprite memory - Some((4, "vram: used blocks (start, size) ours only [], game only [(187, 32), (219, 64), (283, 64), (363, 64)]"))
  * 13,555: sprite memory - Some((555, "vram: used blocks (start, size) ours only [(428, 4)], game only []"))
  * 15,337: RNG - Some((337, "rng: [9f5a0d30, eb8b0d2e] vs game [522ac42b, 94183eb3]"))
  * 16,408: sprite memory - Some((408, "vram: used blocks (start, size) ours only [(375, 4)], game only []"))
  * 17,000: screen fade - Some((0, "fade: black true vs game false"))
  * 19,005: object set - Some((5, "missing: obj #5 sprite 110 has no entity (ours there: sprite Some(108))"))
  * 24,000: camera - Some((11, "cam: (80, 4.013671875) vs game (80, 4.0390625)"))
* `r067_071_14-2` (067_k03_goku158): 693 / 2,415 (Some((693, "cam: (36.1904296875, 126.6875) vs game (36.4619140625, 126.919921875)"))) -> **1,309** / 1,106 / 0; 1 / 2 tiles exact
  * 1,374: sprite memory - Some((374, "vram: used blocks (start, size) ours only [(499, 4), (503, 1)], game only []"))
