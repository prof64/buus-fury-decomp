"""alloclog.py SEGMENT.json a b [state]: replay a segment and print every object-tile allocator call in video frames
a..b (alloc / free / purge / acquire / resize / restore) with its size or handle, caller and return chain"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

NAMES = {0x0801124C: 'alloc', 0x0801142A: 'free', 0x08011732: 'purge', 0x08011818: 'acquire', 0x0801183E: 'resize',
         0x08011E94: 'restore'}

head, frames = load_doc(sys.argv[1])
a, b = int(sys.argv[2]), int(sys.argv[3])
st = sys.argv[4] if len(sys.argv) > 4 else str(ROOT / 'work' / 'segments' / f"{head['name']}.ss")
e = start_emu(st)
for k in range(a):
    e.set_keys(frames[k]['keys'])
    e.run(1)
for x in NAMES:
    e.breakpoint(x)
for k in range(a, b):
    e.set_keys(frames[k]['keys'])
    e.run(1)
    for h in e.hits():
        r = h['regs']
        pc = r[15] - 2
        sp = r[13]
        stk = [hex(e.r32(sp + 4 * i)) for i in range(12)]
        stk = [s for s in stk if s.startswith('0x80') and len(s) == 9][:3]
        arg = e.r16(r[0] + 0xE) if pc == 0x08011818 else r[1]
        print(k, frames[k]['lf'], NAMES.get(pc, hex(pc)), hex(arg), 'obj', hex(r[0]), 'vt', hex(e.r32(r[0])),
              'lr', hex(r[14] & ~1), stk, flush=True)
