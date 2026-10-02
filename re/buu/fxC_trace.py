"""fxC_trace.py type frames dx dy [snaps] -- force-spawn enemy `type`, pin it at player+(dx,dy) every frame
(player 9999 HP), and log per frame every effect OBJ (fx record match, world pos relative to the ENEMY position,
flips, OBJ mode/prio) plus unmatched raw-ROM 8bpp OBJs. Game tick 0x03000F94 shown for lag.
Output: work/buu/fx/enemy_<type>.txt (+ screenshots work/buu/fx/enemy_<type>_f<N>.png)."""
import sys
from common import *
from forcespawn import spawn
from fxcap import FxIndex, capture
from ents import PLAYER

REC = 0x03001560
t = int(sys.argv[1], 0)
frames = int(sys.argv[2])
dx, dy = int(sys.argv[3]), int(sys.argv[4])
PINONCE = len(sys.argv) > 6 and sys.argv[6] == "1"
snaps = {int(x) for x in sys.argv[5].split(',')} if len(sys.argv) > 5 and sys.argv[5] else set()
e, o, ok = spawn(t)
assert o, 'no enemy'
p = e.r32(PLAYER)
ix = FxIndex()
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
out = []
PD = [int(v) for v in sys.argv[7].split(",")] if len(sys.argv) > 7 else [0, 0]
e.w32(p + 0x1a0, (e.r32(p + 0x1a0) + (PD[0] << 8)) & 0xffffffff)
e.w32(p + 0x1a4, (e.r32(p + 0x1a4) + (PD[1] << 8)) & 0xffffffff)
for f in range(frames):
    e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
    px, py = s(e.r32(p + 0x1a0)), s(e.r32(p + 0x1a4))
    if f == 0 or not PINONCE: e.w32(o + 0x1a0, (px + (dx << 8)) & 0xffffffff)
    if f == 0 or not PINONCE: e.w32(o + 0x1a4, (py + (dy << 8)) & 0xffffffff)
    e.run(1)
    ex, ey = s(e.r32(o + 0x1a0)) >> 8, s(e.r32(o + 0x1a4)) >> 8
    items = []
    for b in capture(e, ix):
        tag = ('' + ('H' if b['hflip'] else '') + ('V' if b['vflip'] else '') + (f"m{b['mode']}" if b['mode'] else '')
               + (f"p{b['prio']}" if b['prio'] != 3 else '') + ('A' if b['affine'] else ''))
        if 'fx' in b:
            items.append(f"{b['fx'][0][0][3:]}#{b['fx'][0][1]}@{b['wx'] - ex},{b['wy'] - ey}{tag}")
        elif b.get('rom_gfx') and b['rom_gfx'] not in (0x080725F0,):
            items.append(f"raw{b['rom_gfx']:08x}:{b['W']}x{b['H']}@scr{b['x']},{b['y']}{tag}")
    out.append(f"{f} {e.r32(0x03000F94) % 1000} e({ex},{ey}) " + ' | '.join(items))
    if f in snaps:
        e.screenshot(str(WORK / 'fx' / f'enemy_{t:02x}_f{f}.png'), scale=2)
(WORK / 'fx').mkdir(exist_ok=True)
(WORK / 'fx' / f'enemy_{t:02x}.txt').write_text('\n'.join(out))
print('\n'.join(l for l in out if '|' in l or '#' in l or 'raw' in l))
