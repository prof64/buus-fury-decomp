"""pl_trace.py STATE 'seq' [extra_offsets_hex,...] -- per-frame trace of player state idx, pos, EP, HP"""
from pl_common import *
import sys
e=new(sys.argv[1]); keys=parse(sys.argv[2])
extra=[int(x,16) for x in sys.argv[3].split(',')] if len(sys.argv)>3 and sys.argv[3] else []
P=player(e); R=rec(e)
prev=None
for i,k in enumerate(keys):
    e.set_keys(k); e.run(1)
    P=player(e)
    row=(pstate(e), e.r32(P+0x158), e.r32(P+0x15c), e.r16(R+4), e.r16(R), tuple(e.r32(P+o) for o in extra))
    print(i, f'k={k:03x}', row[0], f'x={row[1]/256:.2f} y={row[2]/256:.2f} ep={row[3]} hp={row[4]}', ' '.join(hex(v) for v in row[5]))
