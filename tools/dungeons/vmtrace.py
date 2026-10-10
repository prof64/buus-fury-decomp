"""vmtrace.py RECORDING.json F0 F1: replay a recording (segment.py's replay) and print, per video frame in [F0, F1], the
bytecode VM's opcode fetches (0x08004066: r1 = address of the opcode byte; the byte) and the logic counter - which
opcodes / natives of the running scripts run in which frame."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from segment import load_doc, replay_to  # noqa: E402

src, f0, f1 = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
head, frames = load_doc(src)
e = replay_to(head, frames, f0 - 1)
e.breakpoint(0x08004066)
for k in range(f0, f1 + 1):
    e.set_keys(frames[k]['keys'])
    e.run(1)
    out = []
    for h in e.hits():
        if h['kind'] == 'break':
            pc = h['regs'][1]
            out.append('%x:%02x' % (pc, e.r8(pc)))
    print(k, 'lf', e.r32(0x03000F94), 'depth', e.r32(0x03002A5C), ' '.join(out[:60]))
