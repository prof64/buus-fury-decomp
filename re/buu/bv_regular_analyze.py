"""Aggregate work/buu/ptrace/batch.json: per spawned object class (vtable) -> users, spawn sites, lifetime,
per-frame movement of the +0x20/+0x24 words, hit messages (attack type, mult) and damage events; per enemy
-> damage events by (attack type, mult) with exact-match counts."""
import json
from collections import Counter, defaultdict
from common import *

COSMETIC = {'0x804c904': 'damage number popup', '0x804cf3c': 'explosion/spark effect (0x0802A81C)'}
b = json.loads((WORK / 'ptrace' / 'batch.json').read_text())
cls = defaultdict(lambda: dict(users=set(), sites=Counter(), lived=[], steps=Counter(), sends=Counter()))
per_enemy = {}
for t, r in b.items():
    dmg = Counter()
    ok = Counter()
    for run in r['runs']:
        for s in run.get('spawns', []):
            vt, site, n = s
            cls[vt]['users'].add(t)
            cls[vt]['sites'][site] += n
        for s in run.get('sends', []):
            lr, svt, atk, mult, world, kind, n = s
            if svt:
                cls[svt]['sends'][(lr, atk, mult, kind)] += n
        for d in run.get('damage', []):
            k = (d['atk'], d['mult'], d['kind'], d['sub'])
            dmg[k] += 1
            ok[k] += d['ok']
        for vt, objs in run.get('objs', {}).items():
            for o in objs:
                cls[vt]['lived'].append(o['lived'])
                fr = o['frames']
                for a, c in zip(fr, fr[1:]):
                    cls[vt]['steps'][(c[8] - a[8], c[9] - a[9])] += 1
    per_enemy[t] = dict(name=r['name'], damage={f'{k[0]} mult {k[1]} kind {k[2]} sub {k[3]}': f'{ok[k]}/{v} exact'
                                                for k, v in dmg.items()})

if __name__ == '__main__':
    for vt, c in sorted(cls.items()):
        lv = sorted(c['lived'])
        print(vt, COSMETIC.get(vt, ''), 'users', sorted(c['users']), 'sites', dict(c['sites']))
        print('    lived', lv[:12], '... max', max(lv) if lv else None)
        print('    steps (+0x20,+0x24 deltas)', c['steps'].most_common(6))
        print('    sends', dict(c['sends']))
    print()
    for t, p in per_enemy.items():
        print(t, p['name'], p['damage'])
