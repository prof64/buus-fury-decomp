"""Tour UI screens, log LZ decompressions (0x03000040) and image blits (0x0803f922), screenshot each step.

tour.py NAME STATE|boot step... ; step = keys separated by ','  e.g. START:60  R:40  'A:30,DOWN:20'
"""
import sys, json
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/ui')
from common import *
from cap import drive, OUT

name = sys.argv[1]
if sys.argv[2] == 'boot':
    e = emu()
else:
    e = emu(sys.argv[2])
bp1 = e.breakpoint(0x03000040)
bp2 = e.breakpoint(0x0803f922)
log = []
for i, step in enumerate(sys.argv[3:]):
    drive(e, step.split(','))
    for h in e.hits():
        r = h['regs']
        if h['pc'] in (0x03000040, 0x03000044, 0x03000048):
            src = r[1]
            hdr = (e.r32(src), e.r32(src + 4)) if 0x08000000 <= src < 0x0a000000 else None
            log.append(dict(step=i, kind='lz', dst=hex(r[0]), src=hex(src), hdr=hdr, lr=hex(h['lr'])))
        else:
            log.append(dict(step=i, kind='blit', r0=hex(r[0]), r1=hex(r[1]), r2=hex(r[2]), r3=hex(r[3]), lr=hex(h['lr'])))
    e.screenshot(OUT / f'{name}_{i}.png', scale=1)
    e.save_file(OUT / f'{name}_{i}.ss')
seen = set()
for l in log:
    k = (l['kind'], l.get('src'), l.get('r1'), l['lr'])
    if k in seen:
        continue
    seen.add(k)
    print(l)
