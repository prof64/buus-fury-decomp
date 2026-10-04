"""airt.py SEGDIR NAME [FRAMES]: every enemy's AI stack (enemy +0x238: +4 depth, +8 {runtime, def} pairs) and the
runtime objects reachable from each level's runtime (pointers to heap objects whose first word is a ROM vtable),
per frame for FRAMES frames (changes only) - to map node runtimes' fields"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
n = int(sys.argv[3]) if len(sys.argv) > 3 else 1
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))


def dump():
    out = []
    c = e.r32(0x03001C4C + 4)
    for k in range(min(c, 255)):
        o = e.r32(0x03001C4C + 8 + 4 * k)
        if e.r32(o) != 0x0804C484:
            continue
        st = o + 0x238
        depth = e.r32(st + 4)
        out.append(f'enemy {o:x} type {(e.r32(o + 0x224) - 0x08711274) // 28} pos {e.r32(o + 0x1A0) / 256} {e.r32(o + 0x1A4) / 256}')
        for lvl in range(min(depth, 6)):
            rt, df = e.r32(st + 8 + 8 * lvl), e.r32(st + 12 + 8 * lvl)
            out.append(f'  lvl {lvl} state {(df - 0x0870F514) // 8:#x} rt {rt:x}')
            seen = set()

            def walk(r, ind):
                if r in seen or not (0x02000000 <= r < 0x02040000):
                    return
                seen.add(r)
                ws = [e.r32(r + 4 * i) for i in range(12)]
                out.append('    ' + '  ' * ind + f'{r:x}: ' + ' '.join(f'{x:x}' for x in ws))
                for x in ws[1:]:
                    if 0x02000000 <= x < 0x02040000 and e.r32(x) >> 24 == 8:
                        walk(x, ind + 1)
            walk(rt, 0)
    return out


prev = None
for f in range(-1, n):
    if f >= 0:
        e.set_keys(keys[f])
        e.run(1)
    cur = dump()
    if cur != prev:
        print(f'== frame {f}')
        print('\n'.join(cur))
    prev = cur
