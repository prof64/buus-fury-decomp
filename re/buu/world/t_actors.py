"""List live world objects (vtable, position) after warping to an area; print exported actors for comparison."""
import sys, json
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from warp import warp
base, key = sys.argv[1], sys.argv[2]
e = emu('buu_snakeway_start.ss')
w, m = map(int, key.split('-'))
print('warp', warp(e, w, m, 200, 200, frames=400))
e.run(1)
W = u32(0x080716B4)
for L in (u32(0x080716B0), W + 4, W + 0x40C):
    lp = L
    for k in range(min(e.r32(L), 256)):
        ob = e.r32(L + 4 + 4 * k)
        print(hex(lp), hex(ob), hex(e.r32(ob)), (e.r32(ob + 0x1a0) >> 8, e.r32(ob + 0x1a4) >> 8))
areas = {a['key']: a for a in json.load(open(base + '/world/areas.json'))['areas']}
o = json.load(open(f'{base}/maps/{areas[key]["maps"][0]}/objects.json'))
for a in o['areas']:
    if a['key'] == key:
        for x in a['objects']:
            if x['type'] in ('npc', 'enemy'):
                print(x['type'], x['pos'], x['rom']['def'], x['condition'], x['behaviour'][0]['op'] if x['behaviour'] else '')
