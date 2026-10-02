"""gfx_probe.py SCENARIO FRAME [FRAME...]: run an objects_* scenario (rec_objects.py) to the given frames and dump the
field objects, the camera and every visible OAM entry (+ a screenshot work/field_gfx/<scenario>_<frame>.png)."""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rec_objects as R  # noqa: E402
from rec_objects import emu, u32  # noqa: E402

SZ = {(0, 0): (8, 8), (0, 1): (16, 16), (0, 2): (32, 32), (0, 3): (64, 64), (1, 0): (16, 8), (1, 1): (32, 8),
      (1, 2): (32, 16), (1, 3): (64, 32), (2, 0): (8, 16), (2, 1): (8, 32), (2, 2): (16, 32), (2, 3): (32, 64)}
OUT = os.path.join(R.ROOT, 'work', 'field_gfx')


def oam(e):
    o = e.oam()
    out = []
    for i in range(128):
        a0, a1, a2 = struct.unpack_from('<HHH', o, i * 8)
        if (a0 >> 8) & 3 == 2:
            continue
        w, h = SZ.get((a0 >> 14, a1 >> 14), (0, 0))
        y = a0 & 0xff
        x = a1 & 0x1ff
        if x >= 256:
            x -= 512
        if y >= 160:
            y -= 256
        out.append(dict(i=i, x=x, y=y, w=w, h=h, tile=a2 & 0x3ff, pal=a2 >> 12, prio=(a2 >> 10) & 3,
                        bpp8=(a0 >> 13) & 1, mode=(a0 >> 10) & 3, hf=(a1 >> 12) & 1, vf=(a1 >> 13) & 1))
    return out


def cam(e):
    c = u32(0x080707E8)
    return e.r32(c) >> 10, e.r32(c + 4) >> 10


def run(name, frames):
    sc = next(s for s in R.SCENARIOS if s['name'] == name)
    e = emu(sc['state'])
    sc['setup'](e)
    keys = []
    for k, n in sc['input']:
        keys += [R.keymask(k)] * n
    os.makedirs(OUT, exist_ok=True)
    want = set(frames)
    for f in range(max(frames) + 1):
        e.set_keys(keys[f] if f < len(keys) else 0)
        e.run(1)
        if f in want:
            print(f'== frame {f} cam {cam(e)}')
            for o in R.objects(e):
                print('  obj', o)
            for o in oam(e):
                print('  oam', o)
            e.screenshot(os.path.join(OUT, f'{name}_{f}.png'), scale=1)
    return e


if __name__ == '__main__':
    run(sys.argv[1], [int(x) for x in sys.argv[2:]])
