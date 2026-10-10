"""Item / pickup icons for all three games (ROM tables; names from the games' own string tables).

LoG2  item table 0x086ADE24, 50 x 56 bytes: +4 u16 name string id (table 0x080E2840, ASCII),
      +8 and +0x18 OBJ descriptors {u32 w, u32 h, u16 attr0, u16 attr1, u32 gfx}; +0x28 / +0x2C more
      webfoot-lz blobs (32x32 8bpp menu icon). Field-pickup icons use the OBJ palette, menu icons the menu
      BG palette 0x081D4F50.
Buu   supplies 0x083BD2F8 (83 x 52 bytes; +4 u16 name id) and equipment 0x086F82EC (162 x 44 bytes; +0 u16
      name id); icon = first OBJ descriptor in the record (16x16 8bpp, webfoot-lz). Strings: UTF-16 table
      0x0806BCE8.
LoG1  item table 0x085C224C, 34 x 24 bytes: +0 u16 w, +2 u16 h, +8 u32 -> {u32 palette, u32 raw 4bpp tiles}.
"""
import struct
from gba import Rom, palette, wf_decompress
from render import obj_image
from export import hx
from misc_export import single_entity
import names

OBJ_PAL = {'log2': 0x81DA6C8, 'buu': 0x805652C}


def _cstr(R, a):
    o = a & 0x1FFFFFF
    return R.d[o:R.d.index(b'\0', o)].decode('latin1')


def _ustr(R, a):
    o = a & 0x1FFFFFF
    s = []
    while True:
        c = struct.unpack_from('<H', R.d, o)[0]
        if not c or len(s) > 80:
            break
        s.append(chr(c))
        o += 2
    return ''.join(s)


def _blob_images(R, p, pal):
    """Decompress a webfoot blob and guess its geometry from the size."""
    # icons, like portraits, are stored as linear (row-major) 8bpp bitmaps
    from PIL import Image
    d, _ = wf_decompress(R.d, p & 0x1FFFFFF)
    side = {256: 16, 1024: 32}.get(len(d))
    if not side:
        return None
    im = Image.new('RGBA', (side, side))
    im.putdata([pal[c] + (255,) if c else (0, 0, 0, 0) for c in d])
    return im


def _valid_blob(R, p):
    return R.isptr(p) and not p & 3 and R.u32(p) in (1, 2) and R.u32(p + 4) in (128, 256, 512, 1024)


def log2_items():
    R = Rom('log2')
    obj = palette(R.bytes(OBJ_PAL['log2'], 512), 256)
    menu = palette(R.bytes(0x81D4F50, 512), 256)
    out = []
    for i in range(50):
        r = 0x86ADE24 + 56 * i
        nm = _cstr(R, R.u32(0x080E2840 + 4 * R.u16(r + 4)))
        frames = []
        for off in range(8, 0x30, 4):
            p = R.u32(r + off)
            if _valid_blob(R, p):
                big = R.u32(p + 4) == 1024
                im = _blob_images(R, p, menu if big else obj)
                if im:
                    frames.append((im, im.width // 2, im.height // 2, hx(p)))
        out.append(single_entity('log2', f'item_{i:02d}_{names.slugify(nm)}', nm, 'item', frames, 'items',
                                 {'item_id': i, 'rom': {'record': hx(r), 'palettes': {'16x16': hx(OBJ_PAL['log2']),
                                                                                     '32x32': '0x081D4F50'}},
                                  'notes': 'frames: field pickup sprite(s) 16x16, then 32x32 menu icon'},
                                 conf='high', anim_name='icons'))
    return out


def buu_items():
    R = Rom('buu')
    pal = palette(R.bytes(OBJ_PAL['buu'], 512), 256)
    strt = 0x0806BCE8
    out = []
    for kind, tbl, n, stride, name_off in (('supply', 0x083BD2F8, 83, 52, 4), ('equipment', 0x086F82EC, 162, 44, 0)):
        for i in range(n):
            r = tbl + stride * i
            nm = _ustr(R, R.u32(strt + 4 * R.u16(r + name_off)))
            frames = []
            for off in range(0, stride, 4):
                p = R.u32(r + off)
                if _valid_blob(R, p):
                    im = _blob_images(R, p, pal)
                    if im and not frames:
                        frames.append((im, im.width // 2, im.height // 2, hx(p)))
            if not frames:
                continue
            out.append(single_entity('buu', f'{kind}_{i:03d}_{names.slugify(nm)}', nm, 'item', frames, 'items',
                                     {'item_kind': kind, 'item_id': i, 'rom': {'record': hx(r), 'palette': hx(OBJ_PAL['buu'])}},
                                     conf='high', anim_name='icon'))
    return out


def log1_items():
    R = Rom('log1')
    out = []
    for i in range(34):
        r = 0x085C224C + 24 * i
        w, h = R.u16(r), R.u16(r + 2)
        g = R.u32(r + 8)
        if not R.isptr(g) or not (8 <= w <= 32 and 8 <= h <= 32):
            continue
        pal = palette(R.bytes(R.u32(g), 32), 16)
        tiles = R.bytes(R.u32(g + 4), w * h // 2)
        im = obj_image(tiles, w, h, 4, pal)
        out.append(single_entity('log1', f'item_{i:02d}', f'Item {i}', 'item', [(im, w // 2, h // 2, hx(R.u32(g + 4)))],
                                 'items', {'item_id': i, 'rom': {'record': hx(r), 'palette': hx(R.u32(g)),
                                                                  'tiles': hx(R.u32(g + 4))},
                                           'notes': 'LoG1 stores no item names; see data/log1/items.json for kinds'},
                                 conf='low', anim_name='icon'))
    return out


def export(game):
    return {'log1': log1_items, 'log2': log2_items, 'buu': buu_items}[game]()
