"""Do bosses lose HP when punched? Force-spawn an enemy type next to Goku, keep Goku next to it (facing it),
mash A for N frames, print the enemy HP (obj+0x28C >> 10) over time and the top AI state.

usage: .venv/bin/python tools/battle/buu_boss_hp_check.py <type hex> [frames] [dx]
"""
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/buu'))
from bv_buu_lib import setup, pos  # noqa: E402
from common import KEY  # noqa: E402
from forcespawn import setpos  # noqa: E402
from ents import PLAYER  # noqa: E402

REC = 0x03001560
t = int(sys.argv[1], 16)
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 600
dx = int(sys.argv[3]) if len(sys.argv) > 3 else 24
e, o = setup(t, dx=-dx, dy=0)
if not o:
    print('spawn failed')
    sys.exit(1)
p = e.r32(PLAYER)
e.w16(REC + 0x1c, 999)
hp0 = e.r32(o + 0x28c) >> 10
print('type', hex(t), 'hp', hp0)
cnt = {}
lrs = set()
for f in range(frames):
    bx, by = pos(e, o)
    setpos(e, p, bx - dx, by)
    e.w8(p + 0x10, 3)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    e.set_keys(KEY.A if f % 10 < 2 else 0)
    b1 = e.breakpoint(0x0800E9D8)
    b2 = e.breakpoint(0x0800FA2A)
    e.run(1)
    for h in e.hits():
        if h['regs'][0] == o:
            cnt[h['addr']] = cnt.get(h['addr'], 0) + 1
            if h['addr'] == 0x0800FA2A:
                lrs.add(hex(h['regs'][14]) if len(h['regs']) > 14 else str(h.get('lr')))
    e.clear(b1)
    e.clear(b2)
    if f % 50 == 0:
        sp = e.r32(o + 0x238 + 4)
        top = e.r32(o + 0x238 + 8 * sp) if sp else 0
        print(f, 'hp', e.r32(o + 0x28c) >> 10, 'stack depth', sp)
print('lost', hp0 - (e.r32(o + 0x28c) >> 10), 'ComputeDamage on it', cnt.get(0x0800E9D8, 0), 'ApplyDamage', cnt.get(0x0800FA2A, 0), 'callers', sorted(lrs))
