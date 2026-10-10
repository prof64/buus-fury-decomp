"""ea_sites.py -- summarise every animation / state / SFX call site seen in the ea_rec recordings.

Groups the events of work/buu/eanim/*.json by caller (lr), names the caller's function (node type + vtable slot from
the AI node registry, or a known engine function), and lists which enemy types / AI states / values reach it.
Writes work/buu/eanim/sites.json."""
import glob
import json
import os
from collections import defaultdict, Counter
from common import *
from vtab import entries
from ai_vts import create_vtable

OUT = WORK / 'eanim'
AI = json.load(open(ROOT / 'data' / 'buu' / 'enemy_ai.json'))


def fn_start(a):
    """walk back to the nearest 'push {.., lr}' (Thumb 0xB5xx)"""
    a &= ~1
    for k in range(0, 0x800, 2):
        x = a - k
        h = u16(x)
        if (h & 0xFF00) == 0xB500:
            return x
    return None


FN = {}
for nt in AI['node_types']:
    cf = int(nt['create_fn'], 16)
    vt = create_vtable(cf)
    FN.setdefault(cf & ~1, []).append('%s.create' % nt['name'])
    if vt:
        for s, f in enumerate(entries(vt, 12)):
            if 0x08000000 <= f < 0x08800000:
                FN.setdefault(f & ~1, []).append('%s[%d]' % (nt['name'], s))
KNOWN = {0x0800DE6C: 'tick_once', 0x0800DEB0: 'tick_loop', 0x0800DE94: 'end->SetAnim(0)', 0x0800DED0: 'end->hold',
         0x0800DEE8: 'punch_enter', 0x0800ED18: 'SetState', 0x0800FAB6: 'SetStateId', 0x0800ECF0: 'SetAnim',
         0x0800E050: 'loop_c.slot4->SetAnim(0)'}


def name(lr):
    s = fn_start(lr)
    n = FN.get(s, []) + ([KNOWN[s]] if s in KNOWN else [])
    return s, n


def main():
    sites = defaultdict(lambda: {'kinds': Counter(), 'vals': Counter(), 'types': Counter(), 'ai': Counter()})
    types_seen = set()
    for path in sorted(glob.glob(str(OUT / '*_*.json'))):
        d = json.load(open(path))
        if 'events' not in d:
            continue
        t = d['type']
        types_seen.add(t)
        ai_at = {}
        for k, o in d['objs'].items():
            for r in o['rows']:
                ai_at[(int(k, 16), r[0])] = (o['type'], r[7])
        for ev in d['events']:
            f, kind, obj, val, lr = ev[:5]
            if kind in ('spawn',):
                continue
            s = sites[(kind, lr)]
            s['kinds'][kind] += 1
            v = val if kind != 'state' else '%08x/%08x' % (val, ev[5])
            if kind == 'hold':
                v = '%d,f%d' % (val, ev[5])
            if kind == 'sfx':
                v = '%d v%d' % (val, ev[5])
            s['vals'][str(v)] += 1
            ot, ai = ai_at.get((obj, f), ai_at.get((obj, f - 1), (None, None)))
            s['types'][hex(ot if ot is not None else t)] += 1
            if ai is not None:
                s['ai'][hex(ai)] += 1
    out = []
    for (kind, lr), s in sorted(sites.items(), key=lambda x: (x[0][0], x[0][1])):
        st, nm = name(lr)
        out.append(dict(kind=kind, lr='%08x' % lr, fn='%08x' % st if st else None, names=nm,
                        vals=dict(s['vals'].most_common()), types=dict(s['types'].most_common()),
                        ai=dict(s['ai'].most_common(8))))
    json.dump(out, open(OUT / 'sites.json', 'w'), indent=1)
    for x in out:
        print('%-8s lr %s fn %s %-40s vals %s types %s' % (x['kind'], x['lr'], x['fn'], ','.join(x['names'])[:40],
                                                          list(x['vals'].items())[:6], list(x['types'])[:8]))
    print(len(types_seen), 'types')


if __name__ == '__main__':
    main()
