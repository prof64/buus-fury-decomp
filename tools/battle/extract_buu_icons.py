"""Extract Buu's Fury skill icons (skill table 0x0871DAF8 +4 / +8 icon pointers) as PNGs for the battle HUD.

Each icon is raw 8bpp OBJ tiles (0x200 bytes = 8 tiles) drawn with the global OBJ palette 0x0805652C.
Output: site/public/assets/buu/battle/icons/skill_<id>_<n|alt>.png (+ /tmp/buu_icons_sheet.png preview).
usage: .venv/bin/python tools/battle/extract_buu_icons.py [w_tiles h_tiles]
"""
import json
import os
import struct
import sys
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ROM = open(os.path.join(ROOT, 'roms/buu.gba'), 'rb').read()
PAL = 0x0805652C


def u16(a):
    return struct.unpack_from('<H', ROM, a & 0x1FFFFFF)[0]


def palette(addr):
    out = []
    for i in range(256):
        c = u16(addr + 2 * i)
        r, g, b = c & 31, (c >> 5) & 31, (c >> 10) & 31
        out.append((r * 255 // 31, g * 255 // 31, b * 255 // 31, 0 if i == 0 else 255))
    return out


def icon(addr, wt, ht, pal):
    im = Image.new('RGBA', (wt * 8, ht * 8))
    off = addr & 0x1FFFFFF
    for t in range(wt * ht):
        tx, ty = t % wt, t // wt
        for y in range(8):
            for x in range(8):
                v = ROM[off + t * 64 + y * 8 + x]
                im.putpixel((tx * 8 + x, ty * 8 + y), pal[v])
    return im


def main():
    wt = int(sys.argv[1]) if len(sys.argv) > 1 else 4
    ht = int(sys.argv[2]) if len(sys.argv) > 2 else 2
    skills = json.load(open(os.path.join(ROOT, 'data/buu/moves.json')))['skills']
    pal = palette(PAL)
    outdir = os.path.join(ROOT, 'site/public/assets/buu/battle/icons')
    os.makedirs(outdir, exist_ok=True)
    sheet = Image.new('RGBA', (wt * 8 * 2 + 4, (ht * 8 + 2) * len(skills)), (40, 40, 40, 255))
    for i, s in enumerate(skills):
        for j, key in enumerate(('icon', 'icon2')):
            im = icon(int(s[key], 16), wt, ht, pal)
            im.save(os.path.join(outdir, f"skill_{s['id']}_{'alt' if j else 'n'}.png"))
            sheet.paste(im, (j * (wt * 8 + 2), i * (ht * 8 + 2)), im)
    sheet.resize((sheet.width * 5, sheet.height * 5), Image.NEAREST).save('/tmp/buu_icons_sheet.png')
    print('ok', outdir)


main()
