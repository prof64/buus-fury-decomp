"""Boot from power-on, screenshot + palette every 30 frames; log full-screen LZ loads."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from pathlib import Path
OUT = Path('/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui/out/boot')
OUT.mkdir(exist_ok=True)
e = emu()
bp = e.breakpoint(0x03000040)
for i in range(60):
    e.run(30)
    for h in e.hits():
        r = h['regs']
        if 0x08000000 <= r[1] < 0x0a000000:
            print('frame', h['frame'], 'lz src', hex(r[1]), 'dst', hex(r[0]), 'lr', hex(h['lr']))
    e.screenshot(OUT / f'f{i:03d}.png', scale=1)
    if i in (5, 10, 15, 20, 25, 30, 40, 50):
        e.save_file(OUT / f'f{i:03d}.ss')
    print(i, 'DISPCNT', hex(e.r16(0x04000000)), 'BLDY', e.r16(0x04000054) & 31, 'BLDCNT', hex(e.r16(0x04000050)))
