"""swingtest.py STATE EVENT NODE HITNODE MISSNODE d0 d1: try ('node', EVENT, NODE, d, A, 3) for delays d0..d1 and report
which reaches HITNODE (vs MISSNODE) first"""
import sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent / 'work' / 'superbuu'   # states, key logs (local)
OUT.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent.parent / 'tools' / 'story_playthrough'))
import sbdriver  # noqa
from sbdriver import node_addr
from driver import Driver
from route import start_emu

st, ev, node, hit, miss, d0, d1 = sys.argv[1], sys.argv[2], *map(int, sys.argv[3:8])
for d in range(d0, d1 + 1):
    e = start_emu(str(HERE / st))
    dr = Driver([('node', ev, node, d, 1, 3)])
    while not dr.done():
        e.set_keys(dr.keys(e)); e.run(1)
    bp = e.breakpoint(0x08012538)
    res = None
    for f in range(1500):
        e.set_keys(0); e.run(1)
        for h in e.hits():
            if h['kind'] == 'break':
                if h['regs'][1] == node_addr(ev, hit): res = 'HIT'
                elif h['regs'][1] == node_addr(ev, miss): res = 'miss'
        if res:
            break
        if f % 30 < 2:
            e.set_keys(1); e.run(1)   # advance text boxes
    print(d, res, flush=True)
