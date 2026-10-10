"""Find position variables: move right, compare RAM."""
import numpy as np
from common import *
e = emu('buu_snakeway_start.ss')
e.run(10)
def snap():
    return np.frombuffer(e.ewram() + e.iwram(), dtype=np.uint8).copy()
def addr(i):
    return 0x02000000 + i if i < 0x40000 else 0x03000000 + i - 0x40000
s0 = snap()
e.hold(KEY.RIGHT, 20); e.set_keys(0); e.run(2)
s1 = snap()
e.hold(KEY.RIGHT, 20); e.set_keys(0); e.run(2)
s2 = snap()
e.run(30)
s3 = snap()
# 16-bit view: increasing monotonically during moves, unchanged while idle
v0 = s0.view(np.uint16).astype(int); v1 = s1.view(np.uint16).astype(int); v2 = s2.view(np.uint16).astype(int); v3 = s3.view(np.uint16).astype(int)
m = (v1 > v0) & (v2 > v1) & (v3 == v2) & ((v2 - v0) < 400)
for i in np.nonzero(m)[0]:
    print(hex(addr(2 * i)), v0[i], v1[i], v2[i])
e.screenshot(WORK / 'mv.png')
print('32bit')
v0 = s0.view(np.int32).astype(np.int64); v1 = s1.view(np.int32).astype(np.int64); v2 = s2.view(np.int32).astype(np.int64); v3 = s3.view(np.int32).astype(np.int64)
m = (v1 > v0) & (v2 > v1) & (v3 == v2)
for i in np.nonzero(m)[0]:
    print(hex(addr(4 * i)), hex(v0[i]), hex(v1[i]), hex(v2[i]))
