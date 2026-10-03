"""rngsteps.py SEG.json F0 F1: RNG draws per video frame of a segment recording (rows' rng), frames F0..F1."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
import common_field as cf  # noqa: E402

doc = json.loads(Path(sys.argv[1]).read_text())
rows = doc['frames']
f0, f1 = int(sys.argv[2]), int(sys.argv[3])
print('row keys', list(rows[f0].keys()))
for f in range(f0, f1 + 1):
    a, b = rows[f - 1]['rng'], rows[f]['rng']
    try:
        n = cf.rng_steps(tuple(a), tuple(b), 200)
    except Exception as ex:  # noqa: BLE001
        n = repr(ex)
    r = rows[f]
    print(f, 'draws', n, 'keys', r['keys'], 'p', r.get('p'), 'snd', r.get('snd'), {k: r[k] for k in r if k in ('hp', 'shake', 'text')})
