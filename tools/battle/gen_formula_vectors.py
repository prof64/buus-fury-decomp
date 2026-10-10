"""Synthetic test vectors from the Python reference implementation re/log2/formula.py (which was verified
against the emulator): RNG sequences from seeds and damage cases covering crits, energy, big scales.
Writes data/battle/log2_formula_vectors.json."""
import json, os, random, sys
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, 're/log2'))
from formula import RNG, damage  # noqa: E402

out = {'source': 'tools/battle/gen_formula_vectors.py (re/log2/formula.py reference)', 'rng': [], 'damage': []}
for seed in [(0, 0), (1, 2), (0xDEADBEEF, 0x12345678), (996209374, 2156006575)]:
    r = RNG(*seed)
    out['rng'].append({'state': list(seed), 'next': [r.next() for _ in range(8)]})
random.seed(7)
for i in range(300):
    a, b = random.getrandbits(32), random.getrandbits(32)
    atype = random.choice([0, 0, 1])
    power = random.choice([1024, 1536, 1280, 2048, 64, 102, 3072])
    S = random.randint(1, 100)
    DEF = random.randint(0, 100)
    MULT = random.choice([128, 128, 96, 64, 38, 0])
    scale = random.choice([1024, 1024, 256, 128, 51, 766, 2048])
    raw, dmg, crit = damage(atype, power, S, DEF, MULT, scale, RNG(a, b))
    out['damage'].append(dict(rng=[a, b], type=atype, power=power, S=S, DEF=DEF, MULT=MULT, scale=scale, raw=raw, dmg=dmg, crit=crit))
json.dump(out, open(os.path.join(ROOT, 'data/battle/log2_formula_vectors.json'), 'w'), indent=0)
print('crits', sum(v['crit'] for v in out['damage']))
