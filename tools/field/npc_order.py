"""npc_order.py NAME F: run to frame F of a scenario and print the order of object updates / RNG calls in it."""
import sys
from npc_lib import *
from npc_re import vt
import rec_npc

name, F = sys.argv[1], int(sys.argv[2])
scn = next(s for s in rec_npc.SCENARIOS if s['name'] == name)
e, _, _, _ = rec_npc.goto_f0(scn)
keys = []
for k, n in scn['input']:
    keys += [k] * n
for f in range(F):
    e.set_keys(keys[f])
    e.run(1)
pl = e.r32(PLAYER)
pupd = vt(e.r32(pl), 4)[2] & ~1
print('player', hex(pl), 'update', hex(pupd))
pts = {0x08011192: 'npc_update', pupd: 'player_update', 0x0803FACC: 'rand', 0x0803FADE: 'chance', 0x08011C70: 'send_first',
       0x08011C0C: 'broadcast', 0x0800ECF0: 'SetAnim', 0x08017CE2: 'npc_msg', 0x0803D8A0: 'run_mode'}


def cb(e, pc):
    r = [e.reg(i) for i in range(16)]
    print(' ', pts.get(pc, hex(pc)), 'r0', hex(r[0]), 'r1', hex(r[1]), 'lr', hex(r[14]))


emutools.catch(e, list(pts), 1, keyfn=lambda _: keys[F], cb=cb)
