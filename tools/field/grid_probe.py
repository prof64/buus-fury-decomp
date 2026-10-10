"""grid_probe.py [W-A[-V] ...]: warp into areas and compare the game's live collision grids (walk +0xA8, shot +0xAC,
enemy +0xB0 of the map object 0x030019A4; bitmaps, 32 bytes per row) right after the load with the extracted
content collision (engine/target/tmp/field-content/base or $WF_FIELD_CONTENT) + the known object marks.
Headless, muted."""
import base64
import struct
import json
import os
import sys
from pathlib import Path
from npc_lib import *  # noqa
import rec_npc

MAPOBJ = 0x030019A4
CONTENT = Path(os.environ.get('WF_FIELD_CONTENT', ROOT / 'engine/target/tmp/field-content/base'))


def unrle(s, n=256 * 256):
    b = base64.b64decode(s)
    out = bytearray(n)
    i = p = 0
    cur = 0
    while i < len(b):
        v = sh = 0
        while True:
            c = b[i]
            i += 1
            v += (c & 0x7F) << sh
            sh += 7
            if not c & 0x80 or i >= len(b):
                break
        if cur:
            for k in range(p, min(n, p + v)):
                out[k] = 1
        p += v
        cur ^= 1
    return out


def content(mapid):
    c = json.load(open(CONTENT / 'maps' / mapid / 'collision.json'))
    w, h = c['width'], c['height']
    full = []
    for key in ('walk', 'shot'):
        flat = unrle(c[key], w * h)
        g = bytearray(256 * 256)
        for y in range(min(h, 256)):
            for x in range(min(w, 256)):
                g[y * 256 + x] = flat[y * w + x]
        full.append(g)
    return full


def live(e, off):
    raw = e.read(e.r32(MAPOBJ + off), 32 * 256)
    return bytes((raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1 for ty in range(256) for tx in range(256))


def mapid_of(w, a, v):
    areas = json.load(open(CONTENT / 'world' / 'areas.json'))['areas']
    ar = next(x for x in areas if x['world'] == w and x['area'] == a)
    ms = ar['maps']
    return ms[min(max(v, 1), len(ms)) - 1]


AREA_NATIVES = {0x0800099C: ('open', 1), 0x080009F6: ('block', 0), 0x08000A50: ('open_alt', 1), 0x08000AA0: ('block_alt', 0)}


def goto(w, a, x, y, variant=0, ops=None):
    """warp; `ops` (a list) collects the script area natives 0x2B..0x2E run during the load: (name, value, rect)"""
    e = emu('buu_snakeway_start.ss')
    fix = arm_warp(e, w, a, x, y, variant)
    keys = KEY.RIGHT

    def cb(e, pc):
        st = e.reg(0)
        n = e.r32(st)
        name, v = AREA_NATIVES[pc]
        ops.append((name, v, tuple(struct.unpack('<i', struct.pack('<I', e.r32(st + 4 + 4 * k)))[0] for k in range(n - 4, n))))
    area0 = e.r32(AREA_PTR)
    e.set_keys(KEY.RIGHT)
    for f in range(900):
        fix()
        if ops is None:
            e.run(1)
        else:
            emutools.catch(e, list(AREA_NATIVES), 1, keyfn=lambda _: keys, cb=cb)
        if e.r32(AREA_PTR) != area0:
            break
    else:
        return None
    e.set_keys(0)
    keys = 0
    # first frame with a player in the new area
    for f in range(200):
        if e.r32(PLAYER):
            break
        if ops is None:
            e.run(1)
        else:
            emutools.catch(e, list(AREA_NATIVES), 1, keyfn=lambda _: 0, cb=cb)
    return e


def diffs(a, b):
    return [(i % 256, i // 256, a[i], b[i]) for i in range(256 * 256) if a[i] != b[i]]


VT_TALK = 0x0804C92C
VT_BREAK = 0x0804CE00
VT_ITEM = 0x0804A314


def span(r):
    x0, y0, x1, y1 = r
    return (max(x0 >> 3, 0), max(y0 >> 3, 0), min(256 if x1 >= 2048 else (x1 + 7) >> 3, 256),
            min(256 if y1 >= 2048 else (y1 + 7) >> 3, 256))


def mark(g, r, v):
    tx0, ty0, tx1, ty1 = span(r)
    for ty in range(ty0, ty1):
        for tx in range(tx0, tx1):
            g[ty * 256 + tx] = v


def object_marks(e, g):
    """the marks the area's objects write at creation: talk NPC footprints, breakable boxes, map items' solid box"""
    seen = set()
    marks = []
    for o in all_objects(e):
        if not o or o in seen:
            continue
        seen.add(o)
        vt = e.r32(o)
        if vt == VT_TALK:
            x, y = e.r32(o + 0x1A0) >> 8, e.r32(o + 0x1A4) >> 8
            r = (x, y, x + e.r8(o + 0x19C), y + e.r8(o + 0x19D))
            marks.append(('npc', r))
        elif vt == VT_BREAK:
            r = tuple(struct.unpack('<4i', e.read(o + 0x14, 16)))
            # only a solid prop (prop +0x1C = map prop record byte +0x17, the tile word's top byte) blocks
            prop = e.r32(o + 0x2C)
            marks.append(('breakable', r if prop and e.r32(prop + 0x1C) else None))
        elif vt == VT_ITEM:
            marks.append(('item', None))
    for kind, r in marks:
        if r:
            mark(g, r, 0)
    return marks


def check(w, a, v=0, pos=None):
    x, y = pos or rec_npc.first_pos(w, a)
    ops = []
    e = goto(w, a, x, y, v, ops)
    if e is None:
        return None
    var = e.r8(0x0300197E)
    hdr = e.r32(MAPOBJ + 0x8C)
    mid = mapid_of(w, a, var)
    cw, cs = content(mid)
    exp = bytearray(cw)
    marks = object_marks(e, exp)
    if any(k == 'item' for k, _ in marks):
        # map items: solid (x, y + 8)..(x + 16, y + 16) at the def position (content objects of the area)
        o = json.load(open(CONTENT / 'maps' / mid / 'objects.json'))
        for ar in o['areas']:
            if ar['key'] == f'{w}-{a}':
                for ob in ar['objects']:
                    if ob['type'] == 'item':
                        ix, iy = ob['pos']
                        mark(exp, (ix, iy + 8, ix + 16, iy + 16), 0)
    exps = bytearray(cs)
    for name, val, r in ops:
        if name in ('open', 'block'):
            mark(exp, r, val)
        else:
            mark(exps, r, val)
    lw, ls = live(e, 0xA8), live(e, 0xAC)
    cs = exps
    return {'ops': ops, 'area': f'{w}-{a}', 'variant': var, 'map': mid, 'header': hex(hdr), 'walk_raw': len(diffs(lw, cw)),
            'walk': diffs(lw, exp), 'shot': diffs(ls, cs), 'marks': [(k, r) for k, r in marks]}


if __name__ == '__main__':
    for arg in sys.argv[1:]:
        parts = [int(p) for p in arg.split('-')]
        w, a = parts[0], parts[1]
        v = parts[2] if len(parts) > 2 else 0
        x, y = rec_npc.first_pos(w, a) if hasattr(rec_npc, 'first_pos') else (120, 120)
        e = goto(w, a, x, y, v)
        if e is None:
            print(arg, 'warp failed')
            continue
        var = e.r8(0x0300197E)
        mid = mapid_of(w, a, var)
        cw, cs = content(mid)
        lw, ls, le = live(e, 0xA8), live(e, 0xAC), live(e, 0xB0)
        dw, ds, de = diffs(lw, cw), diffs(ls, cs), diffs(le, cw)
        print(f'{arg} variant {var} map {mid}: walk {len(dw)} shot {len(ds)} enemy {len(de)}')
        print('  walk', dw[:40])
        print('  shot', ds[:20])
