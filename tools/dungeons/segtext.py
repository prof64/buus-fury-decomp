"""segtext.py SEGDIR NAME OBJ F...: the text buffer of text box object OBJ (hex) at the given frames."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, o = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3], 16)
want = sorted(int(x) for x in sys.argv[4:])
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(-1, want[-1] + 1):
    if f >= 0:
        e.set_keys(keys[f])
        e.run(1)
    if f in want:
        b, cur, end = e.r32(o + 0x10), e.r32(o + 0x14), e.r32(o + 0x18)
        cs = []
        a = b
        while a < b + 600:
            c = e.r16(a) if hasattr(e, 'r16') else (e.r32(a & ~3) >> (16 * ((a >> 1) & 1))) & 0xffff
            if c == 0:
                break
            cs.append(chr(c) if 32 <= c < 127 else '<%x>' % c)
            a += 2
        print(f, 'cur', (cur - b) // 2, 'end', (end - b) // 2, 'w', [hex(e.r32(o + 4 * i)) for i in range(7, 12)], ''.join(cs))
