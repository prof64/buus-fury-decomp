"""script_trace_janemba.py [state] [frames] -- boss handover trace: Janemba (type 0x33) dies and its spawn record's
on-defeat script (#450, Janemba -> Super Janemba cutscene) runs.
State work/buu/jan_end_33.ss (bv_janemba_early.py: Janemba force-spawned into a Southwest Forest spawn record with
its own record fields copied, so the defeat hook / script are Janemba's). Janemba is killed through the game's
damage path 0x0800FA2A with an injected lethal hit (traced); the defeat script then runs naturally. The map is not
HFIL, so actor lookups for NPC actors that HFIL would contain may resolve to nothing (commands then dropped)."""
import sys
from common import *
from ents import enemies, PLAYER, TYPE_TAB
from script_trace import Tracer, traced_call

st = sys.argv[1] if len(sys.argv) > 1 else str(WORK / 'jan_end_33.ss')
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 3000
REC = 0x03001560
HIT = 0x0203FE00
e = emu(st)
e.run(2)
e.w32(REC + 0x24, 60000000)
tr = Tracer(e)
ty = lambda o: (e.r32(o + 0x224) - TYPE_TAB) // 28
boss = [o for o in enemies(e) if ty(o) == 0x33]
print('enemies', [hex(ty(o)) for o in enemies(e)], 'boss', [hex(b) for b in boss])
for i in range(0, 0x20, 4):
    e.w32(HIT + i, 0)
e.w32(HIT + 4, 0x400); e.w32(HIT + 8, 0x086B8098); e.w32(HIT + 0xC, e.r32(PLAYER))
e.w32(HIT + 0x18, 0x7FFFFFF)
print('call', traced_call(tr, 0x0800FA2A, (boss[0], HIT, 0, 0)))
S = Shots(e, 'trace_janemba', cols=8)
keep = lambda f: (e.w16(REC + 0x16, 9999), S.snap() if f % 100 == 0 else None)
tr.run(frames, lambda f: KEY.A if f % 30 < 4 else 0, on_frame=keep)
S.write()
print(tr.save('boss_janemba_defeat', {'state': st, 'how': 'Janemba killed by injected lethal hit (0x0800FA2A, traced); '
                                      'defeat script runs naturally; arena = SW Forest stand-in, not HFIL'}),
      tr.summary())
