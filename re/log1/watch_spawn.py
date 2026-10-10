"""watch_spawn.py MAP START END [mode]: warp to MAP and log accesses to [START,END) during load."""
import sys
from common import *
from warp import run_until_loadmap, default_entrance

m = int(sys.argv[1])
s, en = int(sys.argv[2], 0), int(sys.argv[3], 0)
mode = sys.argv[4] if len(sys.argv) > 4 else 'w'
e = emu('log1_menus.ss')
assert run_until_loadmap(e, KEY.A, 3000)
e.set_reg(0, m)
e.set_reg(1, default_entrance(m))
e.set_keys(0)
e.watch(s, en, mode)
e.run(int(sys.argv[5]) if len(sys.argv) > 5 else 10)
seen = {}
for h in e.hits():
    k = (h['pc'], h['addr'] - s)
    if k not in seen:
        seen[k] = h
for (pc, off), h in sorted(seen.items(), key=lambda x: (x[1]['frame'], x[0][1])):
    print(f"pc={pc:08x} lr={h['lr']:08x} +{off:03x} w{h['width']} {h['old']:#x}->{h['new']:#x} f={h['frame']} r0-r7={' '.join('%x'%r for r in h['regs'][:8])}")
