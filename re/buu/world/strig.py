"""Disassemble script-trigger bytecode (ctor 0x08007387 def+0x10) for some areas; count natives."""
import sys
from collections import Counter
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
from scriptdis import dis_script
AREAS = 0x0808E2E0
nat = Counter()
shown = 0
lim = int(sys.argv[1]) if len(sys.argv) > 1 else 6
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    for k in range(u8(a + 3)):
        d = u32(u32(a + 0x10) + 8 * k + 4)
        if u32(d) != 0x08007387:
            continue
        code = u32(d + 0x10)
        lines = dis_script(code, 200)
        for _, s in lines:
            if s.startswith('native'):
                nat[s.split()[1]] += 1
        if shown < lim:
            shown += 1
            print(f'--- {u8(a)}-{u8(a+1)} def {d:08x} rect ({s16(d+8)},{s16(d+10)})-({s16(d+12)},{s16(d+14)}) code {code:08x}')
            for x, s in lines:
                print('   ', s)
print(nat.most_common(40))
