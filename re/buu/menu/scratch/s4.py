import sys; sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import *
c = Cap('buu_snakeway_start.ss')
REC = 0x03001560
pts = int(sys.argv[1]) if len(sys.argv) > 1 else 5
c.e.w16(REC + 0x2c, pts)
probe = lambda e: dict(lf=e.r32(0x03000F94))
c.run(['-:2', 'START:1', '-:80', 'R:1', '-:30', 'L:1', '-:30'], probe)
prev = None
line = []
for i, f in enumerate(c.frames):
    d = f['lf'] - prev if prev is not None else 0
    prev = f['lf']
    line.append(str(d))
print(''.join(line))
