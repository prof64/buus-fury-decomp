"""heapnode.py SEGMENT.json a b [state]: every WanderToRandomPoint path node created in video frames a..b
(create 0x0803BD64: the heap block `0x0803D5B8(0x24)` returns at 0x0803BD6C): its address and the leftover words
it starts with (+0x0C / +0x10 waypoint, +0x18 counter byte, +0x1C / +0x20 goal); and every heap alloc / free of a
block at that address seen before it (alloc 0x0803D5B8, free 0x0803D63C), to find the previous occupant"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

head, frames = load_doc(sys.argv[1])
a, b = int(sys.argv[2]), int(sys.argv[3])
st = sys.argv[4] if len(sys.argv) > 4 else str(ROOT / 'work' / 'segments' / f"{head['name']}.ss")
e = start_emu(st)
for k in range(a):
    e.set_keys(frames[k]['keys'])
    e.run(1)
e.breakpoint(0x0803BD6C)
e.breakpoint(0x0803D63C)
for k in range(a, b):
    e.set_keys(frames[k]['keys'])
    e.run(1)
    for h in e.hits():
        r = h['regs']
        pc = r[15] - 2
        if pc == 0x0803BD6C:
            o = r[0]
            w = [e.r32(o + 0xC), e.r32(o + 0x10), e.r8(o + 0x18), e.r32(o + 0x1C), e.r32(o + 0x20)]
            print(k, 'wander node', hex(o), 'leftover wp', hex(w[0]), hex(w[1]), 'counter', w[2], 'goal', hex(w[3]), hex(w[4]), flush=True)
        else:
            print(k, 'free', hex(r[0]), 'lr', hex(r[14] & ~1), flush=True)
