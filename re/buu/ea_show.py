"""ea_show.py file.json [obj] -- print an ea_rec recording as runs of (raw, frame, state vtable, ai) + events."""
import json
import sys

d = json.load(open(sys.argv[1]))
want = sys.argv[2] if len(sys.argv) > 2 else None
ev = {}
for x in d['events']:
    ev.setdefault(x[0], []).append(x)
for k, o in d['objs'].items():
    if want and k != want:
        continue
    print('== obj', k, 'type', hex(o['type']), 'first', o['first'], 'rows', len(o['rows']))
    prev = None
    oid = int(k, 16)
    for r in o['rows']:
        f, raw, fr, tick, dr, st, stvt, ai, x, y, hp, z, hit = r
        key = (raw, fr, stvt, ai, dr)
        evs = [e for e in ev.get(f, []) if e[2] in (0, oid) or e[1] in ('spawn',)]
        if key != prev or evs:
            es = ' '.join('%s(%s%s@%x)' % (e[1], e[3] if e[1] != 'state' else hex(e[3]),
                                          ',' + str(e[5]) if len(e) > 5 else '', e[4]) for e in evs)
            print('%4d raw %2d fr %d tick %4d dir %d st %08x ai %s x %d hp %d z %d | %s' % (
                f, raw, fr, tick, dr, stvt, hex(ai) if ai is not None else None, x >> 8, hp, z, es))
            prev = key
