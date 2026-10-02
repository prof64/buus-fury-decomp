"""Find the dialog object + its surface; trace writes into the surface pixels and ROM reads by the writer."""
import sys, struct
from collections import Counter
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
e = emu('buu_menus.ss')
bp = e.breakpoint(0x0800ada0)
e.press(KEY.A, 3, 3)
e.run(10)
h = e.hits()
e.clear(bp)
obj = h[0]['regs'][0]
print('dialog obj', hex(obj), 'lr', hex(h[0]['lr']))
for o in range(0, 0xc0, 16):
    print(hex(o), ' '.join(f'{e.r32(obj + o + k):08x}' for k in range(0, 16, 4)))
surf = obj + 0x80
print('surface words', [hex(e.r32(surf + k)) for k in range(0, 0x30, 4)])
