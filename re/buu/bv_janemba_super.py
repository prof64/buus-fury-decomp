"""bv_janemba_super.py frames [hpfrac] [player_dx] -- Super Janemba (0x7A) live log.
Logs AI states (0x0800AA4A goto / push), boss position jumps > 8 px in one frame (teleports, with the
offset from the player), projectile objects (vtable 0x0804B9B4): spawn frame, per-frame speed, lifetime,
and damage calls on the player (ComputeDamage 0x0800E9D8, attack type + mult + raw/final read at the
routine's return 0x0800EABE through catch-free breakpoint registers is not possible, so the player's HP
drop is reported instead with the player's END/END2 kept fixed)."""
import sys
import math
from common import *
from bv_janemba_spawn import jspawn
from ents import enemies, PLAYER

REC = 0x03001560
PROJ_VT = 0x0804B9B4
frames = int(sys.argv[1]) if len(sys.argv) > 1 else 1500
hpfrac = float(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2] != '-' else None
pdx = int(sys.argv[3]) if len(sys.argv) > 3 else None
e, o, ok = jspawn(0x7A, (0x0800AA4A, 0x0800AA9C, 0x0801191A, 0x0800E9D8))
e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
e.w32(REC + 0x24, 60000000)
print('spawned', hex(o), ok)
if hpfrac is not None:
    e.w32(o + 0x28c, int(25840 * hpfrac) << 10)
p = e.r32(PLAYER)


def sv(v):
    return v - (1 << 32) if v & 0x80000000 else v


def pos(x, off=0x1a0):
    return sv(e.r32(x + off)) / 256, sv(e.r32(x + off + 4)) / 256


if pdx is not None:
    bx, by = pos(o)
    e.w32(p + 0x1a0, int((bx + pdx) * 256) & 0xffffffff); e.w32(p + 0x1a4, int(by * 256) & 0xffffffff)
PIN = int(sys.argv[4]) if len(sys.argv) > 4 else None
last = pos(o)
proj = {}
W = 0x03001c4c
for f in range(frames):
    if PIN is not None:
        bx, by = pos(o)
        e.w32(p + 0x1a0, int((bx + PIN) * 256) & 0xffffffff); e.w32(p + 0x1a4, int(by * 256) & 0xffffffff)
    hp_before = e.r16(REC + 0x16)
    e.run(1)
    hp_after = e.r16(REC + 0x16)
    if hp_after < hp_before:
        print(f'{f:5d} PLAYER HP -{hp_before - hp_after}')
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    for h in e.hits():
        a, r = h['addr'], h['regs']
        if a in (0x0800AA4A, 0x0800AA9C) and r[0] - 0x238 == o:
            print(f'{f:5d} {"goto" if a == 0x0800AA4A else "push"} state {r[1]:#x}  boss {pos(o)} player {pos(p)}')
        elif a == 0x0801191A:
            vt = e.r32(r[1])
            if vt == PROJ_VT:
                proj[r[1]] = dict(t0=f, last=None, steps=[])
                print(f'{f:5d} projectile spawn {r[1]:#x} dir {e.r8(r[1] + 0x10)} vel {sv(e.r32(r[1] + 0x40))},{sv(e.r32(r[1] + 0x44))}')
        elif a == 0x0800E9D8 and r[0] == p:
            print(f'{f:5d} dmg call on player, hit {r[1]:#x}')
    now = pos(o)
    d = math.hypot(now[0] - last[0], now[1] - last[1])
    if d > 8:
        pp = pos(p)
        print(f'{f:5d} JUMP {d:.1f}px boss {now} rel-to-player ({now[0] - pp[0]:.1f},{now[1] - pp[1]:.1f})')
    last = now
    n = e.r32(W + 4)
    live = set(e.r32(W + 8 + 4 * i) for i in range(min(n, 200)))
    for q, s in list(proj.items()):
        if q in live and e.r32(q) == PROJ_VT:
            xy = pos(q, 0x20)
            if s['last'] is not None:
                s['steps'].append((round(xy[0] - s['last'][0], 3), round(xy[1] - s['last'][1], 3)))
            s['last'] = xy
            s['box'] = [sv(e.r32(q + 0x28 + 4 * k)) for k in range(4)]
        else:
            print(f'{f:5d} projectile {q:#x} gone after {f - s["t0"]} f, steps {sorted(set(s["steps"]))[:4]}, box {s.get("box")} pos {s["last"]}')
            del proj[q]
