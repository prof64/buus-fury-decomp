"""objlist.py SEGMENT.json FRAME [state]: replay a segment to video frame FRAME and print the world list (world
0x03001C4C: count +4, object pointers +8..): index, pointer, vtable, tile handle (+0xC), tile bytes (+0xE), and the
used allocator blocks no listed object's +0xC names"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

W = 0x03001C4C
head, frames = load_doc(sys.argv[1])
n = int(sys.argv[2])
st = sys.argv[3] if len(sys.argv) > 3 else str(ROOT / 'work' / 'segments' / f"{head['name']}.ss")
e = start_emu(st)
for k in range(n + 1):
    e.set_keys(frames[k]['keys'])
    e.run(1)
cnt = e.r32(W + 4)
seen = set()
for i in range(cnt):
    o = e.r32(W + 8 + 4 * i)
    h, b = e.r16(o + 0xC), e.r16(o + 0xE)
    seen.add(h)
    print(i, hex(o), 'vt', hex(e.r32(o)), 'handle', hex(h), 'bytes', hex(b))
print('blocks:')
for i in range(138):
    p = W + 0x694 + 10 * i
    if e.r8(p) == 1:
        s, z = e.r16(p + 2), e.r16(p + 4)
        print(' ', hex(s * 2), 'units', z, '' if s * 2 in seen else '<- no object +0xC')
