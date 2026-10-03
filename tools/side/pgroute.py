"""Post-game legs from the REAL post-game state (work/side/pg_start.ss: tools/side/postgame_state.py - the real
route's own frame after the ending's flag 149, flying on the world map; Goku 158, Gohan 141, Vegeta 150).

Each leg starts from pg_start.ss (or a (state, goals) entry: another leg's end) and is driven by the HFIL driver (tools/side/hfil.py SideDriver: 'fly' = the
world-map entry rect, 'goto' = the area maze, charsel = the save point's Switch Character); the leg's end state and
key log go to work/side/pg_<leg>.ss / pg_<leg>.keys.json, then `--record` records it as a master
(work/side/seg/pg_<leg>.json) and cuts it (work/side/pgseg/<leg>/).

    .venv/bin/python tools/side/pgroute.py LEG [LEG ...] [--record] [--cap N]
"""
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import hfil  # noqa: E402

A_, U_ = 1, 64
D = ROOT / 'work' / 'side'
PY = str(ROOT / '.venv' / 'bin' / 'python')
# a level gate: stand inside its trigger rect and press A (facing up: walk up against it first)
GATE = [('keys', [(U_, 20, 5), (A_, 3, 60)]), ('events',), ('wait', 30)]
# the save point: Switch Character, `moves` steps along the row (Goku, Gohan, Goten, Trunks, Vegeta, ...)
LEGS = {
    # Southwest Forest 8-2: the tier-2 gate, Goku L100 (s517, flag 163)
    'g82': [('fly', 849, 1785, 31, 23), ('wait', 30), ('goto', '8-2', 104, 58, False), *GATE],
    # Northern Mountains 13-1: the post-game per-visit gate, Goku L135 (s518, flag 187 for the visit), walk through
    'g131': [('fly', 721, 809, 143, 135), ('wait', 30), ('goto', '13-1', 104, 100, False), *GATE,
             ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # East District 2-8's save point: Gohan; 2-33: the hidden dungeon's per-visit gate, Gohan L140 (s518)
    'g233': [('fly', 2632, 1465, 80, 71), ('wait', 30), ('goto', '2-8', 150, 290, False), *hfil.charsel(1),
             ('wait', 20), ('goto', '2-33', 226, 92, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # 8-1's save point: Vegeta; 8-7: the per-visit gate, Vegeta L120 (s518), through to 8-8
    # (the way to 8-7 is behind 8-2's tier-2 gate: Goku opens it first)
    'g87': [('fly', 849, 1785, 31, 23), ('wait', 30), ('goto', '8-2', 104, 58, False), *GATE,
            ('goto', '8-1', 145, 165, False), *hfil.charsel(4),
            ('wait', 20), ('goto', '8-7', 448, 72, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # 8-1's save point: Gohan; 8-1's take-off pad (A) -> the world map; Diablo Desert 11-1: the tier-2 gate, Gohan
    # L100 (s517, flag 170)
    'g111': [('fly', 849, 1785, 31, 23), ('wait', 30), ('goto', '8-1', 145, 165, False), *hfil.charsel(1),
             ('wait', 20), ('walk', 280, 262), ('keys', [(A_, 3, 240)]), ('fly', 1505, 985, 143, 159), ('wait', 30),
             ('goto', '11-1', 224, 192, False), *GATE, ('keys', [(U_, 60, 10)]), ('wait', 30)],
    # the hidden dungeon past 2-33 (from the g233 leg's end, Gohan in 2-34): the Key in 2-47 (s118, via 2-39), then
    # the locked chest in 2-46 (s117: the Z-Fighter Exhibit, item 71, flag 193)
    # (2-39's house door: the exit rect x 192..208 - walk up from x 200; the planner's edge spot at x 189 is blocked)
    'hidden': ('pg_g233.ss', [('goto', '2-39', 200, 180), ('walk', 201, 180), ('keys', [(U_, 60, 5)]), ('wait', 30),
                              ('goto', '2-47', 104, 120), ('walk', 90, 96), ('face', 'U'), ('keys', [(A_, 3, 60)]), ('events',),
                              ('wait', 30), ('walk', 104, 128),
                              # 2-35's rock (breakable, 3 punches, hit box 249..279 x 128..144) splits the area: the
                              # way to 2-40 .. 2-46 is past it
                              ('goto', '2-35', 230, 128), ('walk', 230, 128), ('face', 'R'),
                              ('keys', [(A_, 2, 14)] * 5), ('wait', 30),
                              ('openwall', '2-35', [(x, y) for x in range(31, 35) for y in range(16, 18)]),
                              ('goto', '2-46', 64, 105), ('keys', [(A_, 3, 60)]), ('events',),
                              ('wait', 30)]),
}


def start_of(leg):
    v = LEGS[leg]
    return (D / v[0], list(v[1])) if isinstance(v, tuple) else (D / 'pg_start.ss', list(v))


def record(leg):
    keys = json.loads((D / f'pg_{leg}.keys.json').read_text())
    keys = keys['keys'] if isinstance(keys, dict) else keys
    kf = D / f'k_pg_{leg}.json'
    kf.write_text(json.dumps({'keys': keys}))
    env = dict(os.environ, SEGMENT_DIR=str(D / 'seg'))
    r = subprocess.run([PY, str(ROOT / 'tools/segments/segment.py'), 'record', '--state', str(start_of(leg)[0]),
                        '--keys', str(kf), '--frames', str(len(keys)), '--name', f'pg_{leg}'],
                       env=env, cwd=ROOT, capture_output=True, text=True)
    print(leg, (r.stdout.strip().splitlines() or [r.stderr[-300:]])[-1], flush=True)
    out = D / 'pgseg' / leg
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob('*'):
        old.unlink()
    subprocess.run([PY, str(ROOT / 'tools/superbuu/mkcuts.py'), str(D / 'seg' / f'pg_{leg}.json'), str(out),
                    '--prefix', f'pg_{leg}', '--min', '100'], env=env, cwd=ROOT, capture_output=True, text=True)
    kf.unlink()


if __name__ == '__main__':
    a = sys.argv[1:]
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 30000
    for leg in [x for x in a if x in LEGS]:
        st, goals = start_of(leg)
        hfil.run(str(st), goals, str(D / f'pg_{leg}.ss'), cap, str(D / f'pg_{leg}.keys.json'),
                 True, marks=False)
        if '--record' in a:
            record(leg)
