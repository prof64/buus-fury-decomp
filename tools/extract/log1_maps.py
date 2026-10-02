"""LoG1 map rendering.

Map table 0x081CA09C: u32 pointers to 0x34-byte map records:
  +0 u16 screens_w, u16 screens_h      +4 u16 tiles_w (=30*sw), u16 tiles_h (=20*sh)
  +8/+0xC/+0x10  per-plane collision/flag layers (not graphics)
  +0x14 entrance list     +0x18 / +0x1C misc tables
  +0x20/+0x24/+0x28  three BG planes: u32[sw*sh] -> screens of 30x20 raw GBA text screen entries (0x4B0 bytes)
  +0x2C palette (BG, 256 colours)     +0x30 tile block list (ptr, count) pairs
Tiles/palette are taken from VRAM/palette RAM after the game loaded the map; screens are read from ROM.
"""
import struct
from PIL import Image
from gba import Rom, bgr555
from wf_maps import tile_cache

MAP_TABLE = 0x081CA09C


def map_count(R):
    n = 0
    while R.isptr(R.u32(MAP_TABLE + 4 * n)):
        n += 1
    return n


def render_map(e, R, mi):
    m = R.u32(MAP_TABLE + 4 * mi)
    sw, sh = R.u16(m), R.u16(m + 2)
    io = e.read(0x04000000, 0x60)
    vram = e.vram()
    pr = e.palram()
    pal = [bgr555(struct.unpack_from('<H', pr, i * 2)[0]) for i in range(256)]
    planes = {}
    for k, off in enumerate((0x20, 0x24, 0x28)):
        lst = R.u32(m + off)
        if not R.isptr(lst):
            continue
        bg = k + 1                     # BG0 is the HUD; world planes are BG1..BG3
        cnt = struct.unpack_from('<H', io, 8 + 2 * bg)[0]
        charbase = ((cnt >> 2) & 3) * 0x4000
        bpp = 8 if cnt & 0x80 else 4
        get = tile_cache(vram, charbase, bpp, pal)
        img = Image.new('RGBA', (sw * 240, sh * 160), (0, 0, 0, 0))
        for sy in range(sh):
            for sx in range(sw):
                sp = R.u32(lst + 4 * (sy * sw + sx))
                if not R.isptr(sp):
                    continue
                ents = struct.unpack('<600H', R.bytes(sp, 0x4B0))
                for ty in range(20):
                    for tx in range(30):
                        se = ents[ty * 30 + tx]
                        t = get(se & 0x3FF, se >> 12)
                        if se & 0x400:
                            t = t.transpose(Image.FLIP_LEFT_RIGHT)
                        if se & 0x800:
                            t = t.transpose(Image.FLIP_TOP_BOTTOM)
                        img.alpha_composite(t, (sx * 240 + tx * 8, sy * 160 + ty * 8))
        planes[bg] = (img, {'priority': cnt & 3, 'bpp': bpp, 'charbase': charbase, 'screens': [sw, sh],
                            'rom_screen_list': lst})
    comp = Image.new('RGBA', (sw * 240, sh * 160), pal[0] + (255,))
    for bg, (img, info) in sorted(planes.items(), key=lambda kv: (-kv[1][1]['priority'], -kv[0])):
        comp.alpha_composite(img)
    return planes, comp, m
