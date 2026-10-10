"""watchat.py state frame start end [mode] [nframes] [keys] -- run to absolute frame, then watch."""
import sys
from common import *
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
target = int(sys.argv[2])
a = int(sys.argv[3], 16); b = int(sys.argv[4], 16)
mode = sys.argv[5] if len(sys.argv) > 5 else 'w'
nf = int(sys.argv[6]) if len(sys.argv) > 6 else 1
keys = 0
if len(sys.argv) > 7 and sys.argv[7] != '-':
    for ch in sys.argv[7].split('+'):
        keys |= getattr(KEY, ch)
e.set_keys(keys)
while e.frame < target:
    e.run(1)
e.watch(a, b, mode)
e.run(nf)
for h in e.hits():
    print(f"f={h['frame']} pc={h['pc']:08x} lr={h['lr']:08x} addr={h['addr']:08x} {h['access']}{h['width']} old={h['old']:x} new={h['new']:x} r0-7={[hex(x) for x in h['regs'][:8]]}")
