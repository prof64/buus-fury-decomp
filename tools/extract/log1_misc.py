"""LoG1 portraits and item icons.

Portraits: u32[47] at 0x0815A55C -> record {u32 palette (16 colours), u32 data}; data is GBA BIOS
Huffman (4-bit) wrapping BIOS RLE, giving 512 bytes = 32x32 4bpp tiles (1D order). Shown as OBJ in the
dialogue box (traced: decoded to EWRAM, copied into the OBJ tile buffer 0x0200B024 by 0x08005134).
"""
from gba import Rom, palette, bios_decompress
from render import obj_image
from export import hx
from misc_export import single_entity
import names

PORTRAIT_TABLE = 0x0815A55C
PORTRAIT_COUNT = 47


def portraits():
    R = Rom('log1')
    N = names.load('log1')
    pn = N.d.get('portraits', {})
    out = []
    seen = {}
    for i in range(PORTRAIT_COUNT):
        rec = R.u32(PORTRAIT_TABLE + 4 * i)
        if rec in seen:
            continue
        seen[rec] = i
        pal = palette(R.bytes(R.u32(rec), 32), 16)
        d = bios_decompress(R.d, R.u32(rec + 4) & 0x1FFFFFF)
        im = obj_image(d, 32, 32, 4, pal)
        info = pn.get(str(i), {})
        nm = info.get('name', f'Portrait {i}')
        slug = f'portrait_{i:02d}' + ('_' + names.slugify(info['name']) if 'name' in info else '')
        out.append(single_entity('log1', slug, nm, 'portrait', [(im, 0, 0, hx(R.u32(rec + 4)))], 'portraits',
                                 {'portrait_index': i,
                                  'table_indices': [k for k in range(PORTRAIT_COUNT)
                                                    if R.u32(PORTRAIT_TABLE + 4 * k) == rec],
                                  'rom': {'record': hx(rec), 'palette': hx(R.u32(rec)), 'gfx': hx(R.u32(rec + 4)),
                                          'gfx_format': 'BIOS Huffman(4-bit) -> BIOS RLE -> 32x32 4bpp tiles'}},
                                 conf=info.get('confidence', 'low')))
    return out


def all_assets(extra):
    import items_export
    return portraits() + items_export.export('log1')
