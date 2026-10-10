"""bp.py state addr [frames] [keys] [max] -- breakpoint and print regs."""
import sys
from common import *
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
a = int(sys.argv[2], 16)
frames = int(sys.argv[3]) if len(sys.argv) > 3 else 2
keys = 0
if len(sys.argv) > 4 and sys.argv[4] != '-':
    for ch in sys.argv[4].split('+'):
        keys |= getattr(KEY, ch)
mx = int(sys.argv[5]) if len(sys.argv) > 5 else 10
e.set_keys(keys)
e.breakpoint(a)
e.run(frames)
hs = e.hits()
print(len(hs), 'hits')
for h in hs[:mx]:
    r = h['regs']
    print(f"f={h['frame']} " + ' '.join(f'r{i}={r[i]:08x}' for i in range(16)))
    sp = r[13]
    st = e.read(sp, 64)
    print('   stack:', ' '.join(f'{int.from_bytes(st[j:j+4],"little"):08x}' for j in range(0, 64, 4)))
