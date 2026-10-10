"""Capture helpers: screenshot + register dump + VRAM tile sheets.

cap.py STATE NAME [keys...]   keys like START:60 A:30 DOWN:20 wait:60
"""
import sys, struct
from pathlib import Path
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from PIL import Image

OUT = Path(__file__).resolve().parent / 'out'
OUT.mkdir(exist_ok=True)


def bgr(c):
    return ((c & 31) * 255 // 31, ((c >> 5) & 31) * 255 // 31, ((c >> 10) & 31) * 255 // 31)


def regs(e):
    d = e.r16(0x04000000)
    s = [f'DISPCNT={d:04x}']
    for i in range(4):
        s.append(f'BG{i}CNT={e.r16(0x04000008 + 2 * i):04x} hofs/vofs?')
    s.append(f'BLDCNT={e.r16(0x04000050):04x} BLDALPHA={e.r16(0x04000052):04x} WININ={e.r16(0x04000048):04x} WINOUT={e.r16(0x0400004a):04x}')
    return '\n'.join(s)


def tilesheet(e, base, n, bpp, palbase, bank=0, cols=32):
    v = e.vram(); pr = e.palram()
    pal = [bgr(struct.unpack_from('<H', pr, palbase + 2 * i)[0]) for i in range(256)]
    tsz = 64 if bpp == 8 else 32
    rows = (n + cols - 1) // cols
    im = Image.new('RGB', (cols * 8, rows * 8), (255, 0, 255))
    px = im.load()
    for t in range(n):
        o = base + t * tsz
        if o + tsz > len(v):
            break
        for y in range(8):
            for x in range(8):
                if bpp == 8:
                    c = v[o + y * 8 + x]
                else:
                    b = v[o + y * 4 + x // 2]
                    c = (b >> (4 * (x & 1))) & 15
                    if c:
                        c += bank * 16
                if c:
                    px[(t % cols) * 8 + x, (t // cols) * 8 + y] = pal[c]
    return im


def dump(e, name):
    e.screenshot(OUT / f'{name}.png', scale=1)
    (OUT / f'{name}.txt').write_text(regs(e))
    print(name, regs(e))


def drive(e, keys):
    for k in keys:
        n, f = k.split(':')
        f = int(f)
        if n == 'wait':
            e.run(f)
        else:
            m = 0
            for part in n.split('+'):
                m |= getattr(KEY, part)
            e.press(m, 3, f)


if __name__ == '__main__':
    st, name = sys.argv[1], sys.argv[2]
    e = emu(st)
    drive(e, sys.argv[3:])
    dump(e, name)
    e.save_file(OUT / f'{name}.ss')
