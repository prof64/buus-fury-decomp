"""script_mode_probe.py state [frames] -- show the run_mode stack (0x03000F90: +0 current mode, +8 depth,
+0xC.. saved modes) with vtables, the VM context stack (0x03001034), and a pc histogram over a few frames."""
import sys
from collections import Counter
from pathlib import Path
from common import *
from emutools import cur_pc

st = sys.argv[1]
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 2
e = emu(st if (STATES / st).exists() or Path(st).is_absolute() else WORK / st)
e.run(1)
M = 0x03000F90
d = e.r32(M + 8)
print('mode cur', hex(e.r32(M)), 'vt', hex(e.r32(e.r32(M))), 'depth', d, 'frame counter', e.r32(M + 4))
for i in range(min(d, 16)):
    m = e.r32(M + 0xC + 4 * i)
    print('  saved', i, hex(m), 'vt', hex(e.r32(m)) if m else '')
n = e.r32(0x03001034)
print('vm depth', n, [hex(e.r32(0x03001034 + 4 + 4 * i)) for i in range(min(n, 8))], 'params', hex(e.r32(0x03001030)))
c = Counter()
for f in range(frames):
    s = e.frame
    while e.frame == s:
        c[cur_pc(e) & ~0xFF] += 1
        e.step()
print([(hex(a), k) for a, k in c.most_common(12)])
