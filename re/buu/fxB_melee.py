"""fxB_melee.py mode [frames] [enemy_hp] -- effect OBJs while the player punches a pinned enemy (mode punch),
dashes (dash), blocks (block: enemy pinned, R held), or idles while being hit (hurt).
Per frame: fx records (positions relative to the player and to enemy #0)."""
import sys
import json
from common import *
from combat import setup, pin, REC
from fxcap import FxIndex, capture

mode = sys.argv[1]
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 120
ehp = float(sys.argv[3]) if len(sys.argv) > 3 else 9999
st = sys.argv[4] if len(sys.argv) > 4 else 'buu_fight_swforest.ss'
e = emu(st)
p, en = setup(e)
e.w8(p + 0x10, 3)
ix = FxIndex()
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
log = []
for f in range(frames):
    if en and mode in ('punch', 'block', 'hurt', 'kill'):
        if mode != 'kill' or f < 2:
            pin(e, en[0], p, 20, 0)
        if mode != 'kill':
            e.w32(en[0] + 0x28c, int(ehp * 1024))
        elif f == 0:
            e.w32(en[0] + 0x28c, int(ehp * 1024))
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    k = 0
    if mode in ('punch', 'kill'):
        k = KEY.A if f % 8 < 3 else 0
    elif mode == 'dash':
        k = KEY.RIGHT if (f < 3 or 6 <= f < 60) else 0
    elif mode == 'block':
        k = KEY.R
    e.set_keys(k)
    e.run(1)
    px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
    ex, ey = (s(e.r32(en[0] + 0x1a0)) >> 8, s(e.r32(en[0] + 0x1a4)) >> 8) if en else (0, 0)
    fr = []
    for o in capture(e, ix):
        if 'fx' in o:
            fr.append(f"{o['fx'][0][0][3:]}#{o['fx'][0][1]}@p{o['wx'] - px},{o['wy'] - py}/e{o['wx'] - ex},{o['wy'] - ey}"
                      + ('H' if o['hflip'] else '') + ('V' if o['vflip'] else '') + (f"m{o['mode']}" if o['mode'] else ''))
        elif o.get('rom_gfx') and o['rom_gfx'] != 0x80725f0:
            fr.append(f"raw{o['rom_gfx']:x}{o['W']}x{o['H']}@s{o['x']},{o['y']}")
    hp = e.r32(en[0] + 0x28c) / 1024 if en else 0
    if fr:
        print(f, e.r32(0x03000f94) % 1000, f'php{e.r16(REC + 0x16)} ehp{hp:.0f}', ' | '.join(fr))
