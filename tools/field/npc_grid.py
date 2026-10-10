"""npc_grid.py NAME: compare the live walk / enemy grids at frame 0 with the content collision (marks = differences)."""
import base64
import json
import sys
from npc_lib import *
import rec_npc

MAPOBJ = 0x030019A4


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


def content_grid(w, m, variant):
    for suf in ([f'_v{variant}'] if variant and variant > 1 else []) + ['']:
        p = ROOT / 'work' / 'field_content' / 'maps' / f'area_{w:02d}_{m:03d}{suf}' / 'collision.json'
        if p.exists():
            c = json.load(open(p))
            return unrle(c['walk']), p
    return None, None


if __name__ == '__main__':
    name = sys.argv[1]
    scn = next(s for s in rec_npc.SCENARIOS if s['name'] == name)
    e, _, _, _ = rec_npc.goto_f0(scn)
    walk = e.read(e.r32(MAPOBJ + 0xA8), 256 * 256)
    enemy = e.read(e.r32(MAPOBJ + 0xB0), 256 * 256)
    variant = e.r8(0x0300197E)
    cg, path = content_grid(*scn['area'], variant)
    print('content', path, 'walk ones live', sum(walk), 'content', sum(cg))
    diff = [(i % 256, i // 256, walk[i], cg[i]) for i in range(256 * 256) if walk[i] != cg[i]]
    print('walk diffs (tx, ty, live, content):', len(diff), diff[:60])
    ed = [(i % 256, i // 256, enemy[i], cg[i]) for i in range(256 * 256) if enemy[i] != cg[i]]
    print('enemy-layer diffs:', len(ed), ed[:60])
    for o in npc_objects(e):
        x, y = e.r32(o + 0x1A0) >> 8, e.r32(o + 0x1A4) >> 8
        print('npc', hex(o), (x, y), 'tiles', (x >> 3, y >> 3, (x + e.r8(o + 0x19C) + 7) >> 3, (y + e.r8(o + 0x19D) + 7) >> 3))
