"""adv.py in.ss out.ss iters [keys] [snap_every] -- mash a key (default A) and grid-snapshot."""
import sys
from common import *
inp, out, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
keys = sys.argv[4] if len(sys.argv) > 4 else 'A'
every = int(sys.argv[5]) if len(sys.argv) > 5 else max(1, n // 16)
k = 0
for ch in keys.split('+'):
    k |= getattr(KEY, ch)
e = emu(WORK / inp if not inp.startswith('/') else inp)
S = Shots(e, 'grid')
for i in range(n):
    e.press(k, 4, 30)
    if i % every == every - 1:
        S.snap()
S.write()
e.save_file(WORK / out)
print('frame', e.frame)
