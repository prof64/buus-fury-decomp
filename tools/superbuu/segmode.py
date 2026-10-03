"""segmode.py SEGMENT a b [n]: replay a segment and print the current mode (0x03000F90 -> vt) and the first n words
of the mode object / of the object at mode+4, each video frame in a..b"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

name, a, b = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
n = int(sys.argv[4]) if len(sys.argv) > 4 else 12
head, frames = load_doc(ROOT / 'data' / 'segments' / f'{name}.json')
e = start_emu(str(ROOT / 'work' / 'segments' / f"{head['name']}.ss"))
for k in range(b + 1):
    e.set_keys(frames[k]['keys'])
    e.run(1)
    if k >= a:
        m = e.r32(0x03000F90)
        o = e.r32(m + 4) if m else 0
        words = [hex(e.r32(m + 4 * i)) for i in range(n)] if m else []
        sub = [hex(e.r32(o + 4 * i)) for i in range(n)] if 0x02000000 <= o < 0x04000000 else []
        print(k, frames[k]['keys'], hex(m), words, sub)
