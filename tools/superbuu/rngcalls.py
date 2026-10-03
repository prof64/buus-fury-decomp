"""rngcalls.py SEGMENT a b : replay a segment (data/segments/NAME.json, start state work/segments/NAME.ss) and print
every RNG call (0x0803FAAC) with its return chain (lr, and the caller's caller from the stack) in video frames a..b"""
import json
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

name, a, b = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
p = Path(name)
if not p.exists():
    p = ROOT / 'data' / 'segments' / f'{name}.json'
head, frames = load_doc(p)
e = start_emu(str(ROOT / 'work' / 'segments' / f"{head['name']}.ss"))
for k in range(a):
    e.set_keys(frames[k]['keys'])
    e.run(1)
bp = [e.breakpoint(x) for x in (0x0803FAAC, 0x0803FACC, 0x0803FADE)]
for k in range(a, b + 1):
    e.set_keys(frames[k]['keys'])
    e.run(1)
    for h in e.hits():
        sp = h['regs'][13]
        stack = [hex(e.r32(sp + 4 * i)) for i in range(12)]
        st = [s for s in stack if s.startswith('0x80')]
        print(k, "lf", frames[k]["lf"], "pc", hex(h["regs"][15]), "lr", hex(h["regs"][14]), "r0", hex(h["regs"][0]), "stack", st[:6])
