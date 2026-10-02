"""bv_fighters_run.py type frames [dist] [state] [hp_frac] -- run a forced-spawn fight and print the log."""
import sys
from bv_fighters_lib import *

t = int(sys.argv[1], 0)
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 600
dist = float(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3] != '-' else None
state = int(sys.argv[4], 0) if len(sys.argv) > 4 and sys.argv[4] != '-' else None
hpf = float(sys.argv[5]) if len(sys.argv) > 5 else None
e, o, L = fight(t, frames, dist=dist, state=state, hp_frac=hpf)
print('boss', hex(o), 'hp', hp(e, o), 'pos', pos(e, o), 'player', pos(e, e.r32(PLAYER)))
print(summarize(L, o))
