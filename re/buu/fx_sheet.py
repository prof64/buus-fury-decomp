"""fx_sheet.py fx_id|addr ... -- contact sheet of effect records (record index, size, origin) -> work/buu/fx/sheet_*.png"""
import sys
import json
import struct
from PIL import Image, ImageDraw
from common import *
from wf_sprites_shim import OBJ_SIZE

pal_raw = rom()[ra(0x0805652C):ra(0x0805652C) + 512]
PAL = []
for i in range(256):
    c = struct.unpack_from('<H', pal_raw, 2 * i)[0]
    PAL.append(((c & 31) * 255 // 31, ((c >> 5) & 31) * 255 // 31, ((c >> 10) & 31) * 255 // 31))


def rec_img(a):
    ox, oy, w, h, a0, a1, gp = struct.unpack_from('<bbBBHHI', rom(), ra(a))
    W, H = OBJ_SIZE[(a0 >> 14, a1 >> 14)]
    im = Image.new('RGBA', (W, H))
    px = im.load()
    for ty in range(H // 8):
        for tx in range(W // 8):
            t = (ty * (W // 8) + tx) * 64
            for yy in range(8):
                for xx in range(8):
                    c = rom()[ra(gp) + t + yy * 8 + xx]
                    if c:
                        px[tx * 8 + xx, ty * 8 + yy] = PAL[c] + (255,)
    if a1 & 0x1000:
        im = im.transpose(Image.FLIP_LEFT_RIGHT)
    if a1 & 0x2000:
        im = im.transpose(Image.FLIP_TOP_BOTTOM)
    return im, ox, oy, W, H


def run_of(a, n=None):
    out = []
    while True:
        try:
            ox, oy, w, h, a0, a1, gp = struct.unpack_from('<bbBBHHI', rom(), ra(a))
            if (a0 >> 14, a1 >> 14) not in OBJ_SIZE or not (0x08000000 <= gp < 0x09000000):
                break
            W, H = OBJ_SIZE[(a0 >> 14, a1 >> 14)]
            if (w, h) != (W, H):
                break
        except Exception:
            break
        out.append(a)
        a += 12
        if n and len(out) >= n:
            break
    return out


def sheet(addr, n=None, scale=3, name=None):
    recs = run_of(addr, n)
    cell = 72
    cols = 8
    rows = (len(recs) + cols - 1) // cols
    S = Image.new('RGBA', (cols * cell * scale, rows * (cell + 12) * scale), (40, 40, 60, 255))
    d = ImageDraw.Draw(S)
    for i, a in enumerate(recs):
        im, ox, oy, W, H = rec_img(a)
        x0 = (i % cols) * cell * scale
        y0 = (i // cols) * (cell + 12) * scale
        im2 = im.resize((W * scale, H * scale), Image.NEAREST)
        S.paste(im2, (x0 + 4, y0 + 14 * scale // 3 * 3), im2)
        d.text((x0 + 2, y0 + 1), f'{i} {W}x{H} o{ox},{oy}', fill=(255, 255, 0))
    (WORK / 'fx').mkdir(exist_ok=True)
    p = WORK / 'fx' / f'sheet_{name or hex(addr)[2:]}.png'
    S.save(p)
    print(p, len(recs))


if __name__ == '__main__':
    for a in sys.argv[1:]:
        a = a.replace('fx_', '')
        sheet(int(a, 16))
