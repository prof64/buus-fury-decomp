"""bplr.py SEGMENT.json a b ADDR [ADDR..]: replay a segment, print each hit of the breakpoints in video frames a..b with
lr and r0..r3"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

head, frames = load_doc(sys.argv[1])
a, b = int(sys.argv[2]), int(sys.argv[3])
e = start_emu(str(ROOT / 'work' / 'segments' / f"{head['name']}.ss"))
for k in range(a):
    e.set_keys(frames[k]['keys'])
    e.run(1)
for x in sys.argv[4:]:
    e.breakpoint(int(x, 16))
for k in range(a, b):
    e.set_keys(frames[k]['keys'])
    e.run(1)
    for h in e.hits():
        r = h['regs']
        print(k, frames[k]['lf'], 'pc', hex(r[15] - 2), 'lr', hex(r[14] & ~1), 'r0-3', [hex(r[j] & 0xffffffff) for j in range(4)], flush=True)
