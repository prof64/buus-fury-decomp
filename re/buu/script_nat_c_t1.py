"""Experiment: run a native snippet in buu_snakeway_start.ss and screenshot while it blocks.
usage: script_nat_c_t1.py name 'item item ...' [frames] [keys-mash]
items: integers are pushes, nXX = native 0xXX, sXX = setter."""
import sys
from common import *
from script_nat_c_lib import asm, start, run_until_done

name = sys.argv[1]
items = []
for t in sys.argv[2].split():
    if t[0] == 'n':
        items.append(('n', int(t[1:], 16)))
    elif t[0] == 's':
        items.append(('s', int(t[1:], 16)))
    else:
        items.append(int(t))
frames = int(sys.argv[3]) if len(sys.argv) > 3 else 300
mash = int(sys.argv[4], 0) if len(sys.argv) > 4 else 0
state = sys.argv[5] if len(sys.argv) > 5 else 'buu_snakeway_start.ss'
e = emu(state)
e.run(2)
sh = Shots(e, f'script/natc_{name}', cols=6)
sh.snap()
done, r, st = start(e, asm(items))
print('returned immediately' if done else 'BLOCKED', st)
sh.snap()
if not done:
    n = run_until_done(e, frames, keys=lambda f: (mash if f % 20 < 3 else 0), every=max(1, frames // 22), shots=sh)
    print('returned after', n, 'frames')
else:
    for f in range(frames):
        e.set_keys(mash if f % 20 < 3 else 0)
        e.run(1)
        if f % max(1, frames // 10) == 0:
            sh.snap()
print(sh.write())
