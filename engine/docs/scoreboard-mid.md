# Parity scoreboard, every-N-frames cuts

Tree: `3d6c757`. Tiles of 1,000 frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the
segments), `scoreboard.py mid cut 1000` (local, under `work/real/mid/`), `scoreboard.py mid`.

Every area-entry segment that loses more than 1,000 frames on the main board, cut again at its frame 1,000,
2,000, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when
the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a
tile's first divergence; **not covered** when its tile was refused by the importer.

| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |
|---|---|---|---|---|---|---|---|---|---|
| fat | 1 | 9,485 | 396 (4.2%) | 398 (4.2%) | 9,087 | 0 | 9 | 0 | 0 |
| **all** | 1 | 9,485 | 396 (4.2%) | 398 (4.2%) | 9,087 | 0 | 9 | 0 | 0 |

## Per segment: where it really diverges

segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each
tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it

### fat

* `r009_006_4-1` (009_d16_desert): 396 / 9,485 (Some((458, "cam: (36.2890625, 509.953125) vs game (36.3359375, 509.943359375)"))) -> **398** / 9,087 / 0; 0 / 9 tiles exact
  * 1,000: position / facing - Some((0, "pos: #5 sprite 261: (322.5, 360) vs game (322, 360)"))
  * 2,000: position / facing - Some((0, "pos: #11 sprite 69: (736, 386) vs game (736, 386.5)"))
  * 3,000: position / facing - Some((0, "pos: #6 sprite 180: (118.5, 344) vs game (118, 344)"))
  * 4,000: position / facing - Some((0, "pos: #3 sprite 36: (680, 547.5) vs game (680, 548)"))
  * 5,000: position / facing - Some((0, "pos: #3 sprite 36: (648, 530) vs game (648, 530.5)"))
  * 6,000: position / facing - Some((0, "pos: #3 sprite 36: (632, 549) vs game (632, 548.5)"))
  * 7,000: position / facing - Some((0, "pos: #3 sprite 36: (648, 528) vs game (648.5, 528)"))
  * 8,000: position / facing - Some((0, "pos: #3 sprite 36: (615.5, 544) vs game (616, 544)"))
  * 9,002: animation - Some((2, "anim: #7 sprite 239: raw 1 cel 0 tick 66 vs game raw 1 cel 1 tick 497"))
