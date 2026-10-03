"""rngcalls.py SEG.json STATE.ss A B  -- replay a segment's keys from its state on the real game; print every RNG call
(0x0803FAAC) in rows A..B with its caller (lr) and the caller's caller from the stack"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import start_emu  # noqa: E402

rows = []
for ln in open(sys.argv[1]).read().splitlines()[1:]:
    try:
        rows.append(json.loads(ln.strip().rstrip(',')))
    except ValueError:
        pass
e = start_emu(sys.argv[2])
a, b = int(sys.argv[3]), int(sys.argv[4])
bp = None
for i, r in enumerate(rows[:b + 1]):
    if i == a:
        bp = e.breakpoint(0x0803FAAC)
        e.hits()
    e.set_keys(r['keys'])
    e.run(1)
    if bp is not None:
        for h in e.hits():
            regs = h['regs']
            sp = regs[13]
            stack = [e.r32(sp + 4 * k) for k in range(12)]
            callers = [hex(x) for x in stack if 0x08000000 <= x < 0x08800000 and x & 1]
            print(i, 'lr', hex(regs[14]), 'stack', ' '.join(callers[:6]))
