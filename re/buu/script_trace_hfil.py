"""script_trace_hfil.py [x] [y] [pre_frames] [post_frames] -- Janemba fight in its real arena, HFIL (18,14).
Snakeway start; story flag 123 set and 125 cleared (Janemba's spawn condition); warp.py teleport to (18,14);
everything from the arrival on is traced (map-entry / pre-fight scripts run naturally, A mashed). Then Janemba is
killed with an injected lethal hit through 0x0800FA2A (traced) and its defeat script (#450 -> #451/#452) runs
naturally. Writes traces hfil_arrival.json and boss_janemba_handover.json, and work/buu/hfil_janemba_dead.ss."""
import sys
from common import *
from ents import enemies, PLAYER, TYPE_TAB
from script_trace import Tracer, traced_call
from warp import warp

x = int(sys.argv[1]) if len(sys.argv) > 1 else 128
y = int(sys.argv[2]) if len(sys.argv) > 2 else 160
pre = int(sys.argv[3]) if len(sys.argv) > 3 else 1500
post = int(sys.argv[4]) if len(sys.argv) > 4 else 3000
GD, REC, HIT = 0x03001068, 0x03001560, 0x0203FE00


def flag(e, n, on):
    a = GD + 0x55 + (n >> 3)
    v = e.r8(a)
    e.w8(a, (v | (1 << (n & 7))) if on else (v & ~(1 << (n & 7))))


e = emu('buu_snakeway_start.ss')
e.run(2)
flag(e, 123, True)
flag(e, 125, False)
e.w32(REC + 0x24, 60000000)
ok = warp(e, 18, 14, x, y, frames=200)
print('warp', ok, 'area', hex(e.r32(0x03001980)))
mash = lambda f: KEY.A if f % 30 < 4 else 0
S = Shots(e, 'trace_hfil', cols=8)
tr = Tracer(e)
tr.run(pre, mash, on_frame=lambda f: (e.w16(REC + 0x16, 9999), S.snap() if f % 150 == 0 else None))
print(tr.save('hfil_arrival', {'how': 'natural: arrival in HFIL (18,14) via warp.py with flag 123 set / 125 clear',
                               'frames': pre}), tr.summary())
e.save_file(WORK / 'hfil_janemba_alive.ss')
ty = lambda o: (e.r32(o + 0x224) - TYPE_TAB) // 28
print('enemies', [hex(ty(o)) for o in enemies(e)])
boss = [o for o in enemies(e) if ty(o) == 0x33]
tr2 = Tracer(e)
if boss:
    for i in range(0, 0x20, 4):
        e.w32(HIT + i, 0)
    e.w32(HIT + 4, 0x400); e.w32(HIT + 8, 0x086B8098); e.w32(HIT + 0xC, e.r32(PLAYER))
    e.w32(HIT + 0x18, 0x7FFFFFF)
    print('call', traced_call(tr2, 0x0800FA2A, (boss[0], HIT, 0, 0)))
    tr2.run(post, mash, on_frame=lambda f: (e.w16(REC + 0x16, 9999), S.snap() if f % 150 == 0 else None))
    print(tr2.save('boss_janemba_handover', {'how': 'Janemba (HFIL 18,14, natural spawn) killed by an injected lethal hit '
                                             '(0x0800FA2A, traced); defeat cutscene natural, A mashed', 'frames': post}),
          tr2.summary())
    e.save_file(WORK / 'hfil_janemba_dead.ss')
S.write()
