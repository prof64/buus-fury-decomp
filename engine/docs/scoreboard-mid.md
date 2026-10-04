# Parity scoreboard, every-N-frames cuts

Tree: `836475d`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| janemba | 1 | 2,802 | 1,349 (48.1%) | 1,582 (56.5%) | 1,220 | 0 | 2 | 1 | 0 |
| superbuu | 1 | 26,177 | 2,176 (8.3%) | 13,515 (51.6%) | 12,662 | 0 | 25 | 9 | 0 |
| **all** | 2 | 28,979 | 3,525 (12.2%) | 15,097 (52.1%) | 13,882 | 0 | 27 | 10 | 0 |

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
| fat | 388,648 | 380,209 (97.8%) | 0 | 8,439 (2.2%) |
| janemba | 199,703 | 186,071 (93.2%) | 1,220 | 12,412 (6.2%) |
| superbuu | 478,729 | 455,039 (95.1%) | 12,662 | 11,028 (2.3%) |
| **all** | 1,668,052 | 1,615,689 (96.9%) | 13,882 | 38,481 (2.3%) |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### janemba

* `r047_002_18-13` (047_h_h13): 1,349 / 2,802 (Some((1470, "fade: black false vs game true"))) -> **1,582** / 1,220 / 0; 1 / 2 tiles exact
  * 1,240: animation - Some((240, "anim: #3 sprite 280: raw 1 cel 0 vs game raw 2 cel 0 tick 0"))

### superbuu

* `r062_001_20-6` (062_m_inside_buu): 2,176 / 26,177 (Some((2176, "pos: #26 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30) -> **13,515** / 12,662 / 0; 9 / 25 tiles exact
  * 2,176: position / facing - Some((176, "pos: #5 sprite 280: (214.58984375, 289.30078125) vs game (214.484375, 289.30078125)"))
  * 3,965: position / facing - Some((965, "pos: #2 sprite 280: (234.79296875, 230.41796875) vs game (233.7890625, 229.03515625)"))
  * 4,315: position / facing - Some((315, "pos: #2 sprite 280: (182.68359375, 212.33203125) vs game (183.13671875, 211.81640625)"))
  * 5,107: position / facing - Some((107, "pos: #2 sprite 280: (287.90625, 197.44921875) vs game (288.8046875, 196.9375)"))
  * 6,130: position / facing - Some((130, "pos: #2 sprite 280: (224.19921875, 320.34375) vs game (224.36328125, 320.99609375)"))
  * 7,027: position / facing - Some((27, "pos: #2 sprite 280: (234.90625, 266.58203125) vs game (234.20703125, 267.14453125)"))
  * 8,920: position / facing - Some((920, "pos: #2 sprite 280: (197.12109375, 235.3125) vs game (197.921875, 234.61328125)"))
  * 9,036: position / facing - Some((36, "pos: #2 sprite 280: (167.4453125, 270.2734375) vs game (167.1875, 268.875)"))
  * 10,085: position / facing - Some((85, "pos: #2 sprite 280: (181.08984375, 191.49609375) vs game (180.390625, 191.34765625)"))
  * 11,197: position / facing - Some((197, "pos: #2 sprite 280: (161.3203125, 333.640625) vs game (160.61328125, 334.0078125)"))
  * 12,035: position / facing - Some((35, "pos: #2 sprite 280: (164.8828125, 236.5625) vs game (164.359375, 235.86328125)"))
  * 13,127: position / facing - Some((127, "pos: #2 sprite 280: (180.0078125, 149.05078125) vs game (179.8828125, 148.3515625)"))
  * ... 4 more
