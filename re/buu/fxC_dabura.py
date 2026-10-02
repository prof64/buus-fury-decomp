"""fxC_dabura.py state frames dist [snaps] -- Dabura (0x12) forced into AI state `state` (0x7c spit barrage,
0x78 counter path -> dark sphere); logs effect OBJs per frame and the BG/blend registers (DISPCNT, BG0CNT, BLDCNT,
BLDALPHA) + the sphere byte 0x030032E4; screenshots at snaps -> work/buu/fx/dabura_<state>_f<N>.png"""
import sys
from bv_fighters_lib import *
from fxcap import FxIndex, capture
from bv_fighters_lib import spawn2, set_state, pos, setpos

st = int(sys.argv[1], 0)
frames = int(sys.argv[2])
dist = int(sys.argv[3])
snaps = {int(x) for x in sys.argv[4].split(',')} if len(sys.argv) > 4 else set()
e, o, ok = spawn2(0x12)
p = e.r32(PLAYER)
bx, by = pos(e, o)
setpos(e, p, bx - dist, by)
set_state(e, o, st)
ix = FxIndex()
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
out = []
for f in range(frames):
    e.w16(0x03001560 + 0x16, 9999)
    e.run(1)
    ex, ey = s(e.r32(o + 0x1a0)) >> 8, s(e.r32(o + 0x1a4)) >> 8
    io = e.read(0x04000000, 0x60)
    regs = f"disp={io[0] | io[1] << 8:04x} bg0={io[8] | io[9] << 8:04x} bld={io[0x50] | io[0x51] << 8:04x} " \
           f"alpha={io[0x52] | io[0x53] << 8:04x} sph={e.r8(0x030032E4)}"
    items = []
    for b in capture(e, ix):
        if 'fx' in b:
            items.append(f"{b['fx'][0][0][3:]}#{b['fx'][0][1]}@{b['wx'] - ex},{b['wy'] - ey}{'H' if b['hflip'] else ''}")
        elif b.get('rom_gfx') and b['rom_gfx'] != 0x080725F0:
            items.append(f"raw{b['rom_gfx']:08x}:{b['W']}x{b['H']}")
    out.append(f'{f} e({ex},{ey}) {regs} ' + ' | '.join(items))
    if f in snaps:
        e.screenshot(str(WORK / 'fx' / f'dabura_{st:02x}_f{f}.png'), scale=2)
(WORK / 'fx' / f'dabura_{st:02x}.txt').write_text('\n'.join(out))
print('\n'.join(out[::10]))
