"""Convert the regular-enemy damage trace (work/buu/ptrace/batch.json, re/buu/bv_regular_batch.py: 79 force-spawned
enemy types, player held at 3 offsets, every ComputeDamage to the player) into data/battle/buu_enemy_hits.json.

Rows: [type, kind, sub, power, mult, S, POW, E, E2, actual, count]  (identical rows deduplicated, count = occurrences).
The RNG state was not recorded: a test checks that `actual` is one of the formula's outputs for some rand(26) value
(crit or not; resist 128, or 0 during the player's hurt i-frames).

usage: python3 tools/battle/gen_buu_enemy_hits.py
"""
import json
import os
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
batch = json.load(open(os.path.join(ROOT, 'work/buu/ptrace/batch.json')))
moves = json.load(open(os.path.join(ROOT, 'data/buu/moves.json')))
power = {int(a['address'], 16): a['power'] for a in moves['attack_types']}

rows = Counter()
total = ok = 0
for t, r in batch.items():
    for run in r['runs']:
        for d in run.get('damage', []):
            if not d.get('to_player', True):
                continue
            total += 1
            ok += bool(d['ok'])
            rows[(int(t, 16), d['kind'], d['sub'], power[int(d['atk'], 16)], d['mult'], d['S'], d['POW'], d['E'], d['E2'],
                  d['actual'])] += 1
hits = [list(k) + [n] for k, n in sorted(rows.items())]
out = dict(source='work/buu/ptrace/batch.json (re/buu/bv_regular_batch.py, headless mGBA) via tools/battle/gen_buu_enemy_hits.py',
           note='enemy->player ComputeDamage events; row = [type, kind, sub, power, mult, S, POW, E, E2, actual (1/1024 HP), count]; '
                f'{total} events, {ok} predicted exactly by re/buu/damage_model.py at record time; RNG state not stored',
           fields=['type', 'kind', 'sub', 'power', 'mult', 'S', 'POW', 'E', 'E2', 'actual', 'count'],
           hits=hits)
path = os.path.join(ROOT, 'data/battle/buu_enemy_hits.json')
json.dump(out, open(path, 'w'), separators=(',', ':'))
print('events', total, 'ok', ok, 'rows', len(hits), '->', path)
