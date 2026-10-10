"""Undecoded map-header words (+0x24..+0x50) and area words for some areas."""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
AREAS = 0x0808E2E0
want = set(sys.argv[1:])
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    key = f'{u8(a)}-{u8(a+1)}'
    if want and key not in want:
        continue
    hdr = u32(u32(a + 0x2c))
    print(key, f'area+8={u32(a+8):x} +28={u32(a+0x28):x} +34={u32(a+0x34):x} hdr {hdr:08x}:',
          ' '.join(f'{o:02x}={u32(hdr + o):08x}' for o in (0x2c, 0x30, 0x34, 0x38, 0x3c, 0x40, 0x44, 0x48, 0x4c, 0x50)))
