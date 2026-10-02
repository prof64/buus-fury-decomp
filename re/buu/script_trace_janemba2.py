"""script_trace_janemba2.py [frames] [vx] [vy] -- Janemba -> Super Janemba handover, complete run.
Starts from work/buu/hfil_janemba_alive.ss (script_trace_hfil.py: HFIL (18,14), natural Janemba spawn). Scene #450
expects a Vegeta NPC (script actor 5 = character record 4's sprite) in the arena: it morphs it into Goku
(native 0x4E), makes Vegeta the player (native 0x95) and pans the camera to the Goku NPC (actor 1). Our warp skipped
whatever put Vegeta there, so a Vegeta object is created with the game's own spawn path (what native 0x35 does:
def = 0x08000158(5); obj = 0x0801111C(0, def, &pos); 0x0801191A(world, obj)) -- injected. Janemba is then killed
by an injected lethal hit (0x0800FA2A, traced); everything after runs naturally with A mashed."""
import sys
from common import *
from emutools import call
from ents import enemies, PLAYER, TYPE_TAB
from script_trace import Tracer, traced_call

frames = int(sys.argv[1]) if len(sys.argv) > 1 else 6000
vx = int(sys.argv[2]) if len(sys.argv) > 2 else 170
vy = int(sys.argv[3]) if len(sys.argv) > 3 else 130
REC, HIT, POS = 0x03001560, 0x0203FE00, 0x0203FDF0
e = emu(str(WORK / 'hfil_janemba_alive.ss'))
e.run(2)
call(e, 0x08000158, (5, 0, 0, 0))   # first injected call after a load does not run (mailbox untouched)
d = call(e, 0x08000158, (5, 0, 0, 0))
e.w16(POS, vx); e.w16(POS + 2, vy)
o = call(e, 0x0801111C, (0, d, POS, 0))
call(e, 0x0801191A, (0x03001C4C, o, 0, 0))   # world object list = [0x08054FBC+0x18]
print('vegeta def', hex(d), 'obj', hex(o), 'actor5 ->', hex(call(e, 0x080001C6, (5, 0, 0, 0))))
ty = lambda q: (e.r32(q + 0x224) - TYPE_TAB) // 28
boss = [q for q in enemies(e) if ty(q) == 0x33]
tr = Tracer(e)
for i in range(0, 0x20, 4):
    e.w32(HIT + i, 0)
e.w32(HIT + 4, 0x400); e.w32(HIT + 8, 0x086B8098); e.w32(HIT + 0xC, e.r32(PLAYER))
e.w32(HIT + 0x18, 0x7FFFFFF)
print('call', traced_call(tr, 0x0800FA2A, (boss[0], HIT, 0, 0)))
S = Shots(e, 'trace_janemba2', cols=10)
tr.run(frames, lambda f: KEY.A if f % 30 < 4 else 0,
       on_frame=lambda f: ([e.w16(REC + 0x68 * i + 0x16, 9999) for i in range(9)], S.snap() if f % 150 == 0 else None))
S.write()
print(tr.save('boss_janemba_handover', {
    'how': 'HFIL (18,14) natural Janemba; Vegeta NPC (actor 5) injected via the game spawn path (stand-in for the '
           'story setup the warp skipped); Janemba killed by injected lethal hit 0x0800FA2A (traced); scene #450 '
           'and successors natural, A mashed', 'frames': frames}), tr.summary())
e.save_file(WORK / 'hfil_after_handover.ss')
