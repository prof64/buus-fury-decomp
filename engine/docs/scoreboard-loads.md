# Area-load board: every area load of the real run on wf-core

Tree: `92bcb81`. Regenerate: `tools/segments/loadboard.py cut` (local, `work/real/loads/`), then
`tools/segments/loadboard.py`. Each main-board segment that is exact, recorded again from its start state through
the area load after it into the next segment's first 60 frames. **load** = the first divergence inside the load's
frames (its length / black / lag: the area-load length model, `sndsim::load_frames`), **after the load** = in the next
segment's first frames (what the load left: positions, camera, RNG), **before the load** = the segment itself
differs when run on past its usual end (none expected). "off" = consecutive frames the first wrong field stays wrong
(a black / fade row: ours longer or shorter by about that many).

| load kind | loads | exact | load | after the load | before the load |
|---|---|---|---|---|---|
| exit | 1511 | 1510 | 0 | 0 | 1 |
| **all** | 1511 | 1510 | 0 | 0 | 1 |

## Misses (for the area-load length model's owner)

segment: load kind, from -> to, load frames (video / logic), where (frames into the load), off, first divergence

* `r010_011_11-13` (before the load): exit, [11, 21, 1] -> [11, 22, 1], 16 / 2, at None, off None - Some((1640, "cam: (240, 34.45703125) vs game (240, 34.71875)"))
