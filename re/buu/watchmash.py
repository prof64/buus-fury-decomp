"""watchmash.py state start end mode frames -- like watchw but mashing A; prints each hit with frame."""
import sys
from common import *
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
a = int(sys.argv[2], 16); b = int(sys.argv[3], 16)
mode = sys.argv[4]
frames = int(sys.argv[5])
e.watch(a, b, mode)
for f in range(frames):
    e.set_keys(KEY.A if f % 30 < 4 else 0)
    e.run(1)
for h in e.hits()[:60]:
    print(f"f={h['frame']} pc={h['pc']:08x} lr={h['lr']:08x} addr={h['addr']:08x} old={h['old']:x} new={h['new']:x} r0-3={[hex(x) for x in h['regs'][:4]]} sp={h['regs'][13]:08x}")
