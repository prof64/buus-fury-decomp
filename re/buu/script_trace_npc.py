"""script_trace_npc.py -- NPC dialog trace (natural): Snakeway start, put Goku next to Gregory (as in
work/buu/talk.ss), face left, press A, mash A through the conversation."""
from common import *
from script_trace import Tracer

e = emu('buu_snakeway_start.ss')
e.run(2)
p = e.r32(0x03001c34)
e.w32(p + 0x1a0, 92 << 8)
e.w32(p + 0x1a4, 179 << 8)
e.hold(KEY.LEFT, 6)
e.set_keys(0)
e.run(10)
tr = Tracer(e)
S = Shots(e, 'trace_npc', cols=6)
keyfn = lambda f: KEY.A if f % 30 < 4 else 0
tr.run(900, keyfn, on_frame=lambda f: S.snap() if f % 60 == 0 else None)
S.write()
print(tr.save('npc_gregory', {'state': 'buu_snakeway_start.ss', 'how': 'natural: Goku placed at (92,179) facing left, A pressed every 30 frames',
                              'frames': 900}), tr.summary())
