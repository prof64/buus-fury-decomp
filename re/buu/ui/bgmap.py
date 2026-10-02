"""bgmap.py STATEFILE BG : print the BG tilemap (32x20) of a captured state"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
e = emu(sys.argv[1])
bg = int(sys.argv[2])
cnt = e.r16(0x04000008 + 2 * bg)
sb = (cnt >> 8) & 31
cb = (cnt >> 2) & 3
print('cnt', hex(cnt), 'sb', sb, 'cb', cb, '8bpp', (cnt >> 7) & 1, 'hofs/vofs n/a')
v = e.vram()
for y in range(21):
    row = [struct.unpack_from('<H', v, sb * 0x800 + (y * 32 + x) * 2)[0] for x in range(32)]
    print(' '.join(f'{r:04x}' for r in row))
