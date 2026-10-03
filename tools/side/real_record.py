"""Record the REAL route's Janemba saga (work/ship/jan, segments.md §8: states + key logs of the real-route agent)
as segment masters and cut them at the area entries. Nothing is re-played by a bot: each leg is its checkpoint state
plus the keys the real route pressed from it.

    .venv/bin/python tools/side/real_record.py [LEG ...]      (default: every leg but the three level grinds)

Masters: work/side/seg/rj_<leg>.json; cuts: work/side/rseg/<leg>/ (tools/superbuu/mkcuts.py).
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
J = ROOT / 'work' / 'ship' / 'jan'
D = ROOT / 'work' / 'side'
PY = str(ROOT / '.venv' / 'bin' / 'python')
# leg: (start state, key log, first frame, frame count or None = to the log's end)
LEGS = {
    'yemma': (ROOT / 'work/ship/fat/dbx/end.ss', 'j00_yemma.keys.json', 0, None),      # prologue 0-96 -> 0-16 -> 1-1
    'hfil': (J / 'j00_yemma.ss', 'j01_hfil.keys.json', 0, None),                        # 1-1 -> 1-5 -> 0-51 -> 18-1
    'goku90': (J / 'j01_hfil.ss', 'j02_goku90.keys.json', 0, None),                     # grind 18-1 <-> 18-12
    'goku120': (J / 'j02_goku90.ss', 'j03_goku120.keys.json', 0, None),                 # grind 18-1 <-> 18-2
    'switch': (J / 'j03_goku120.ss', 'h_switch.keys.json', 0, None),
    'maze': (J / 'h_switch.ss', 'h_maze.keys.json', 0, None),
    'punched': (J / 'h_maze.ss', 'h_maze_punched.keys.json', 0, None),
    'statues': (J / 'h_maze_punched.ss', 'h_statues.keys.json', 0, None),
    'vegeta': (J / 'h_statues.ss', 'h_vegeta.keys.json', 0, None),
    'back1': (J / 'h_vegeta.ss', 'h_back1.keys.json', 0, None),
    'veg0': (J / 'h_back1.ss', 'h_veg0.keys.json', 0, None),
    'veg1': (J / 'h_veg0.ss', 'h_veg1.keys.json', 0, None),                             # grind 18-1 <-> 18-2 (Vegeta)
    'h13': (J / 'h_veg1.ss', 'h_h13.keys.json', 0, None),
    'arena0': (J / 'h_h13.ss', 'j10_arena.keys.json', 0, None),
    'arena': (J / 'j10_arena.ss', 'a_keys_arena_fail.json', 0, 7119),                   # s449, Janemba, Super Janemba
    'gogeta': (J / 'j_sj2.ss', 'a_keys_sj2.json', 0, None),                             # the fusion, Gogeta, s452
}
GRINDS = ('goku90', 'goku120', 'veg1')
legs = sys.argv[1:] or [k for k in LEGS if k not in GRINDS]
(D / 'seg').mkdir(parents=True, exist_ok=True)
for m in legs:
    st, kf, a, n = LEGS[m]
    ks = json.loads((J / kf).read_text())
    ks = ks['keys'] if isinstance(ks, dict) else ks
    ks = ks[a:a + n] if n else ks[a:]
    tmp = D / f'k_real_{m}.json'
    tmp.write_text(json.dumps({'keys': ks}))
    env = dict(os.environ, SEGMENT_DIR=str(D / 'seg'))
    name = f'rj_{m}'
    r = subprocess.run([PY, str(ROOT / 'tools/segments/segment.py'), 'record', '--state', str(st), '--keys', str(tmp),
                        '--frames', str(len(ks)), '--name', name], env=env, cwd=ROOT, capture_output=True, text=True)
    print(m, (r.stdout.strip().splitlines() or [r.stderr[-300:]])[-1], flush=True)
    out = D / 'rseg' / m
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob('*'):
        old.unlink()
    r = subprocess.run([PY, str(ROOT / 'tools/superbuu/mkcuts.py'), str(D / 'seg' / f'{name}.json'), str(out)],
                       env=env, cwd=ROOT, capture_output=True, text=True)
    print(m, 'cuts', len(list(out.glob('*.json'))), r.stderr[-300:], flush=True)
    tmp.unlink()
