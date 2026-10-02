"""script_trace_run.py state frames name [mash|none] [snap_every] -- trace a state while optionally mashing A
(4 of every 30 frames). Screenshots every snap_every frames into work/buu/script/<name>.png (grid)."""
import sys
from pathlib import Path
from common import *
from script_trace import Tracer

st, frames, name = sys.argv[1], int(sys.argv[2]), sys.argv[3]
mode = sys.argv[4] if len(sys.argv) > 4 else 'mash'
snap = int(sys.argv[5]) if len(sys.argv) > 5 else 0
e = emu(st if (STATES / st).exists() or Path(st).is_absolute() else WORK / st)
tr = Tracer(e)
keyfn = (lambda f: KEY.A if f % 30 < 4 else 0) if mode == 'mash' else (lambda f: 0)
S = Shots(e, name, cols=6)


def of(f):
    if snap and f % snap == 0:
        S.snap()


tr.run(frames, keyfn, on_frame=of)
if snap:
    S.write()
print(tr.save(name, {'state': st, 'input': mode, 'frames': frames}), tr.summary())
