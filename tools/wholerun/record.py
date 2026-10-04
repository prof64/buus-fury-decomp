"""record.py [OUT.jsonl] [--frames N]: the whole real run on the emulator from the soft reset (the board's power-on:
`route.start_emu('reset')`), its key logs in order - data/story_playthrough/earth_route_keys.json (title, New Game,
Other World, Earth) then the 73 data/realrun/*.keys.gz stretches - one row per video frame, the fields the app soak
compares: keys, lf (logic counter), rng, area, p (leader x, y, facing, state), depth, hp, zenie, flags (hex, only when
they changed). Each stretch end is checked against data/realrun/manifest.json. Default OUT
work/wholerun/master.jsonl (plain JSON lines: a first line {"stretches": [[name, first row], ...]} is written last,
as OUT.index.json)."""
import gzip
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
from route import start_emu, status, WS, GD, DEPTH  # noqa: E402
from common_field import player_row  # noqa: E402
import realrun  # noqa: E402

a = sys.argv[1:]
out = Path(a[0]) if a and not a[0].startswith('--') else ROOT / 'work' / 'wholerun' / 'master.jsonl'
limit = int(a[a.index('--frames') + 1]) if '--frames' in a else None
out.parent.mkdir(parents=True, exist_ok=True)

LF, RNG = 0x03000F94, 0x03004A5C
s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v
parts = [(name, realrun.pre_keys()[part], None) for _, name, part in realrun.PRE]
man = json.loads((ROOT / 'data' / 'realrun' / 'manifest.json').read_text())
for m in man:
    keys = realrun.unrle(json.loads(gzip.open(ROOT / 'data' / 'realrun' / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
    parts.append((f'{m["n"]:03d}_{m["name"]}', keys, m['end']))

e = start_emu('reset')
fl = None
row_n = 0
index = []
t0 = time.time()
with out.open('w') as fh:
    for name, keys, end in parts:
        index.append([name, row_n])
        for k in keys:
            if limit is not None and row_n >= limit:
                break
            e.set_keys(k)
            e.run(1)
            rc = 0x03001560 + 0x68 * e.r8(GD)
            row = {'keys': k, 'lf': e.r32(LF), 'rng': [e.r32(RNG), e.r32(RNG + 4)], 'area': [e.r8(WS), e.r8(WS + 1), e.r8(WS + 2)],
                   'p': player_row(e), 'depth': e.r32(DEPTH), 'hp': e.r16(rc + 0x16), 'zenie': e.r32(GD + 0x4F4),
                   # the camera object (as the segment rows' `cam`): the harness's menu-close camera step uses it
                   'cam': [s32(e.r32(0x030019A4)), s32(e.r32(0x030019A8))]}
            f2 = e.read(GD + 0x55, 0x80).hex()
            if f2 != fl:
                row['flags'] = f2
                fl = f2
            fh.write(json.dumps(row, separators=(',', ':')) + '\n')
            row_n += 1
        if limit is not None and row_n >= limit:
            break
        if end is not None and status(e) != end:
            raise SystemExit(f'{name}: end state {status(e)} != manifest {end}')
        print(f'{name}: to row {row_n} ({time.time() - t0:.0f}s)', flush=True)
(out.parent / (out.name + '.index.json')).write_text(json.dumps({'rows': row_n, 'stretches': index}))
print('rows', row_n)
