"""Inspect Super Buu 0x77/0x78 spawn records and the force-spawned object."""
from common import *
from forcespawn import spawn_records
from bv_buu_lib import *
from ents import enemies

for s in spawn_records():
    if s['type'] in (0x77, 0x78, 0x76, 0x79):
        r = int(s['record'], 16)
        print(hex(s['type']), s['area'], s['world'], s['map'], s['record'], 'xy', s['x'], s['y'], 'gfx', s['gfx'],
              'cond', s['cond'], 'words', [hex(u32(r + 4 * k)) for k in range(8)])
e, o = setup(0x77, dx=90)
print('obj', hex(o), 'vt', hex(e.r32(o)), 'pos', pos(e, o), 'ctrl', hex(e.r32(o + 0x14)), 'ctrl vt', hex(e.r32(e.r32(o + 0x14))))
st = e.r32(o + 0x238)
print('ai stack obj+0x238 words', [hex(e.r32(o + 0x238 + 4 * k)) for k in range(6)])
for x in enemies(e):
    print(' enemy', hex(x), 'type', hex((e.r32(x + 0x224) - 0x08711274) // 28), pos(e, x))
