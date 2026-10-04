"""cut.py ROW N [OUTDIR]: a full segment of the whole run's chain - the emulator from the reset to ROW, then N rows
recorded with the segment recorder (every actor, camera, sounds: what the segment harness compares) - as
OUTDIR/wr_<ROW>.json + states/wr_<ROW>.ss (default work/wholerun/segs). For a whole-run stop the harness can then show
what the leader-only comparison does not (an NPC's walk, an enemy's state). Headless, muted."""
import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 'tools/script_lockstep', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
import realrun  # noqa: E402
import segment as sg  # noqa: E402
from route import start_emu  # noqa: E402

row, n = int(sys.argv[1]), int(sys.argv[2])
out = Path(sys.argv[3]) if len(sys.argv) > 3 else ROOT / 'work' / 'wholerun' / 'segs'
(out / 'states').mkdir(parents=True, exist_ok=True)
keys = []
for _, name, part in realrun.PRE:
    keys += realrun.pre_keys()[part]
for m in json.loads((ROOT / 'data' / 'realrun' / 'manifest.json').read_text()):
    if len(keys) > row + n:
        break
    keys += realrun.unrle(json.loads(gzip.open(ROOT / 'data' / 'realrun' / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
e = start_emu('reset')
for k in range(row):
    e.set_keys(keys[k])
    e.run(1)
st = out / 'states' / f'wr_{row}.ss'
e.save_file(str(st))
init, objs, rows = sg.record(e, lambda j, _e: keys[row + j] if row + j < len(keys) else None, n)
c = start_emu(str(st))
c0, p0 = c.cycles(), c.r32(0x0300389C)
while c.r32(0x0300389C) == p0:
    c.step()
init['snd_phase'] = c.cycles() - c0
name = f'wr_{row}'
sg.OUT = out
p = sg.save(name, {'kind': 'segment', 'name': name, 'source': {'kind': 'state', 'state': str(st.relative_to(ROOT))}, 'init': init, 'objs': objs, 'frames': rows})
print(p, len(rows), 'rows')
