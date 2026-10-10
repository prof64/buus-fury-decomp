"""Find where dialog text lives at runtime: walk into the snakeway text trigger, then search RAM for UTF-16 text."""
import sys, re
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *

e = emu('buu_snakeway_start.ss')
o = 0x0203DFA4
print('obj', [hex(e.r32(o + 4 * k)) for k in range(8)])
p = e.r32(0x03001C34)
before = e.ewram() + e.iwram()
e.w32(p + 0x1a0, 370 << 8)
e.w32(p + 0x1a4, 190 << 8)
e.run(60)
for k in range(6):
    e.press(KEY.A)
    e.run(20)
e.screenshot('/tmp/dlg.png')
ram = e.ewram()
pat = re.compile(rb'(?:[\x20-\x7e]\x00){8,}')
for m in pat.finditer(ram):
    s = m.group().decode('utf-16-le')
    print(hex(0x02000000 + m.start()), repr(s[:100]))
iw = e.iwram()
for m in pat.finditer(iw):
    print(hex(0x03000000 + m.start()), repr(m.group().decode('utf-16-le')[:100]))
open('/tmp/ewram.bin', 'wb').write(ram)
open('/tmp/iwram.bin', 'wb').write(iw)
