"""segobjs.py SEGMENT k [bp_addr]: replay a segment to video frame k and list the world objects; with bp_addr, print
r0/r1 (and the object list index) whenever bp_addr runs in frame k"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402
from common_field import objs  # noqa: E402

name, k = sys.argv[1], int(sys.argv[2])
p = Path(name)
if not p.exists():
    p = ROOT / 'data' / 'segments' / f'{name}.json'
head, frames = load_doc(p)
e = start_emu(str(ROOT / 'work' / 'segments' / f"{head['name']}.ss"))
for j in range(k):
    e.set_keys(frames[j]['keys'])
    e.run(1)
if len(sys.argv) > 3:
    b = e.breakpoint(int(sys.argv[3], 16))
e.set_keys(frames[k]['keys'])
e.run(1)
if len(sys.argv) > 3:
    for h in e.hits():
        r = h['regs']
        print('bp r0', hex(r[0] & 0xffffffff), 'r1', hex(r[1] & 0xffffffff), 'r4', hex(r[4] & 0xffffffff), 'lr', hex(r[14] & 0xffffffff))
W = 0x03001C4C
n = e.r32(W + 4)
for i in range(min(n, 255)):
    o = e.r32(W + 8 + 4 * i)
    print(i, hex(o), hex(e.r32(o)), [hex(e.r32(o + 4 * q)) for q in range(1, 8)])
for o in objs(e):
    print(o)
