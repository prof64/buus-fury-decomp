"""script_trace_loot.py [state] [n_kills] -- enemy loot-drop trace (script #497 and its sub-scripts).
The enemies of the state are killed through the game's own damage-apply path 0x0800FA2A(obj, hit) (a lethal
hit injected with emutools.call, same as bv_janemba_early.kill_all); death, the drop script and the pickups then
run naturally. The trace starts before the injected hit."""
import sys
from common import *
from script_trace import traced_call
from ents import enemies, PLAYER, TYPE_TAB
from script_trace import Tracer

st = sys.argv[1] if len(sys.argv) > 1 else 'buu_fight_swforest.ss'
nk = int(sys.argv[2]) if len(sys.argv) > 2 else 3
HIT = 0x0203FE00
e = emu(st)
e.run(2)
e.w32(0x03001560 + 0x24, 60000000)   # XP: max level, so no level-up box pauses the game
tr = Tracer(e)
killed = []
tr.run(300)
for o in enemies(e)[:nk]:
    t = (e.r32(o + 0x224) - TYPE_TAB) // 28
    for i in range(0, 0x20, 4):
        e.w32(HIT + i, 0)
    e.w32(HIT + 4, 0x400); e.w32(HIT + 8, 0x086B8098); e.w32(HIT + 0xC, e.r32(PLAYER))
    e.w32(HIT + 0x18, 0x7FFFFFF)
    print("call", traced_call(tr, 0x0800FA2A, (o, HIT, 0, 0)))
    killed.append((hex(t), e.r32(o + 0x28c) / 1024))
    tr.run(400, lambda f: KEY.A if f % 30 < 4 else 0, on_frame=lambda f: e.w16(0x03001560 + 0x16, 9999))
print('killed types', killed)
print(tr.save('loot_drop', {'state': st, 'how': 'death natural after an injected lethal hit (0x0800FA2A); '
                                              'drop scripts run naturally', 'killed_types': killed}), tr.summary())
