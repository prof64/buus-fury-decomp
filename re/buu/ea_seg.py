"""ea_seg.py -- cut ea_rec recordings into animation segments and check the generic tick rules against them.

A segment = one stay of an actor in one state object (obj+0x14): from the frame a SetState (0x0800ED18) put it there
(rows hold the values after that frame's update) until the next SetState for the same actor. Library + CLI summary."""
import glob
import json
import sys
from collections import defaultdict, Counter
from common import *

OUT = WORK / 'eanim'

# state-object vtables -> generic tick rule (see ea_tables.py / ea_ctl.py)
GENERIC = {
    0x0804A77C: ('idle', 3), 0x0804C790: ('idle', 3),
    0x0804A1D0: ('loop', 3), 0x08049E40: ('loop', 3), 0x0804C720: ('loop', None), 0x0804C3A8: ('loop', 3),
    0x0804D784: ('once', None), 0x0804C35C: ('once', 2), 0x0804E0AC: ('once_stop', 3), 0x0804A730: ('once', 5),
    0x0804A188: ('hold', None), 0x0804985C: ('hold', None),
    0x0804A1AC: ('bored', 5),
    0x0804AE14: ('reverse', 3), 0x0804CC80: ('reverse', 3),
}
ONCE_SHIFT = {0x03001B10: 3, 0x03001B18: 4, 0x03001B50: 2, 0x03001B40: 5, 0x03001B30: 3, 0x03001B38: 4}
LOOP_SHIFT = {0x03001B20: 3, 0x03001B28: 4}


def nframes(defp, raw):
    if not (0x08000000 <= defp < 0x0A000000) or raw < 0:
        return 0
    a = u32(defp + 0x50 + 4 * raw)
    return u32(a) if 0x08000000 <= a < 0x0A000000 else 0


def step(kind, shift, n, fr, tick):
    """one tick of the generic rule -> (frame, tick, ended)"""
    if kind in ('once', 'once_stop'):
        tick = (tick + 1) & 0xFFFF
        f = tick >> shift
        if f >= n:
            return fr, tick, True
        return f, tick, False
    if kind == 'loop':
        tick = (tick + 1) & 0xFFFF
        f = tick >> shift
        if f >= n:
            tick, f = 0, 0
        return f, tick, False
    if kind == 'idle':
        tick = (tick + 1) & 0xFFFF
        if n == 2:
            return (1 if (tick & 0xFF) > 0xF0 else 0), tick, False
        f = (tick >> 3) & 0xFF
        if f >= n:
            f, tick = 0, 0
        return f, tick, False
    if kind == 'bored':
        tick = (tick + 1) & 0xFFFF
        f = tick >> 5
        if f >= n:
            return 2, 0x40, False
        return f, tick, False
    if kind == 'reverse':
        tick = tick - 1
        if tick < 0:
            tick = n * 8 - 1
        return (tick >> 3) & 0xFF, tick, False
    if kind == 'hold':
        return fr, tick, False
    raise ValueError(kind)


def load(path):
    return json.load(open(path))


def segments(d):
    """-> list of segments for the recording's enemy objects"""
    out = []
    evs = defaultdict(list)
    for ev in d.get('events', []):
        evs[(ev[2], ev[0])].append(ev)
    lt = d.get('lt')
    lag = set(i for i in range(1, len(lt)) if lt[i] == lt[i - 1]) if lt else set()
    for k, o in list(d.get('objs', {}).items()) + list(d.get('extra', {}).items()):
        oid = int(k, 16)
        rows = [r for r in o['rows'] if r[0] not in lag]   # lag frames: no game update ran
        if rows and (rows[0][8] >> 8) == 0x7000:
            continue   # the area's other spawn records, parked far away by forcespawn (idle all along)
        if not rows:
            continue
        cur = None
        for i, r in enumerate(rows):
            f = r[0]
            es = evs.get((oid, f), [])
            sts = [e for e in es if e[1] == 'state']
            if sts or cur is None or r[6] != cur['stvt']:
                if cur is not None:
                    cur['next'] = [(e[1], e[3], e[4]) for e in es]
                    cur['complete'] = bool(sts)
                    out.append(cur)
                last = sts[-1] if sts else None
                cur = dict(obj=oid, type=o.get('type'), deff=o['def'], start=f, stvt=r[6], st=r[5],
                           entry=[(e[1], e[3], e[4]) + tuple(e[5:]) for e in es], entered=bool(last),
                           enter_lr=last[4] if last else None, rows=[], ai=r[7], complete=False, next=None)
            cur['rows'].append([f, r[1], r[2], r[3], r[4], r[13] if len(r) > 13 else o['def']])
            # raw / hold events inside the segment
            for e in es:
                if e[1] in ('raw', 'hold', 'anim', 'stateid') and not sts:
                    cur.setdefault('inner', []).append((f, e[1], e[3], e[4]))
        if cur is not None:
            out.append(cur)
    return out


def check(seg):
    """simulate the generic rule from the first row; -> (kind, n_checked, mismatch frame or None)"""
    g = GENERIC.get(seg['stvt'])
    if not g:
        return None, 0, None
    kind, shift = g
    if kind in ('once', 'once_stop'):
        shift = ONCE_SHIFT.get(seg['st'], shift)
    if kind == 'loop' and shift is None:
        shift = LOOP_SHIFT.get(seg['st'], 3)
    rows = seg['rows']
    raw = rows[0][1]
    n = nframes(seg['deff'], raw)
    fr, tick = rows[0][2], rows[0][3]
    ended = False
    for i, r in enumerate(rows[1:], 1):
        if r[1] != raw:
            return kind, i, r[0]
        if ended and kind == 'once':
            return kind, i, r[0]   # should have left the state
        fr, tick, ended = step(kind, shift, nframes(r[5], raw) if len(r) > 5 else n, fr, tick)
        if (fr, tick) != (r[2], r[3]):
            return kind, i, r[0]
    return kind, len(rows) - 1, None


if __name__ == '__main__':
    files = sorted(glob.glob(str(OUT / (sys.argv[1] if len(sys.argv) > 1 else '*_*.json'))))
    stat = defaultdict(Counter)
    bad = defaultdict(list)
    custom = Counter()
    for p in files:
        d = load(p)
        for s in segments(d):
            kind, n, mis = check(s)
            if kind is None:
                custom[(s['stvt'], s['type'])] += 1
                continue
            stat[kind]['ok' if mis is None else 'bad'] += 1
            stat[kind]['frames'] += n
            if mis is not None:
                bad[kind].append((p.split('/')[-1], hex(s['obj']), s['start'], mis, s['rows'][:3]))
    for k, v in stat.items():
        print(k, dict(v))
        for b in bad[k][:6]:
            print('   bad', b)
    print('custom state objects:', len(custom))
    by = defaultdict(list)
    for (vt, t), c in custom.items():
        by[vt].append((t, c))
    for vt, l in sorted(by.items()):
        print('  %08x' % vt, l[:10])
