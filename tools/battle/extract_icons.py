"""Extract LoG2 special-move HUD icons (move-type table 0x081D9900 icon_gfx) as PNGs.

Each icon is raw 8bpp OBJ tiles (512 bytes) drawn with the global OBJ palette 0x081DA6C8.
Output: site/public/assets/log2/battle/icons/move_<type>_<n|alt>.png plus a preview sheet in /tmp.
usage: .venv/bin/python tools/battle/extract_icons.py [w_tiles h_tiles]
"""
import json, os, struct, sys
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ROM = open(os.path.join(ROOT, 'roms/log2.gba'), 'rb').read()
PAL = 0x081DA6C8


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
    wt = int(sys.argv[1]) if len(sys.argv) > 1 else 2
    ht = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    moves = json.load(open(os.path.join(ROOT, 'data/log2/moves.json')))['player_moves']
    pal = palette(PAL)
    outdir = os.path.join(ROOT, 'site/public/assets/log2/battle/icons')
    os.makedirs(outdir, exist_ok=True)
    sheet = Image.new('RGBA', (wt * 8 * 2 + 4, (ht * 8 + 2) * len(moves)), (40, 40, 40, 255))
    for i, m in enumerate(moves):
        for j, a in enumerate(m['icon_gfx']):
            im = icon(int(a, 16), wt, ht, pal)
            im.save(os.path.join(outdir, f"move_{m['id']}_{'alt' if j else 'n'}.png"))
            sheet.paste(im, (j * (wt * 8 + 2), i * (ht * 8 + 2)), im)
    sheet.resize((sheet.width * 6, sheet.height * 6), Image.NEAREST).save('/tmp/icons_sheet.png')
    print('ok', outdir)


main()
