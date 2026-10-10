"""Area-name banner reference (headless, muted): forest 8-3 -> 8-2 with the EnterArea name word (0x03001984) cleared,
so the load shows the "Southwest Forest" banner. Per frame after the area load: screenshot, banner OAM entries,
their affine matrices, the frame counter; plus the banner's 240x16 8bpp surface from OBJ VRAM.

  .venv/bin/python engine/crates/wf-render/re/banner_capture.py  ->  work/frontend/banner/
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, KEY  # noqa: E402
from PIL import Image  # noqa: E402

OUT = ROOT / 'work' / 'frontend' / 'banner'
OUT.mkdir(parents=True, exist_ok=True)
PLAYER = 0x03001C34
NAME_WORD = 0x03001984

e = emu('buu_swforest3.ss')
bp = e.breakpoint(0x08008E4E)
p = e.r32(PLAYER)
e.w32(p + 0x1A0, 430 << 8)
e.w32(p + 0x1A4, 515 << 8)
e.w16(NAME_WORD, 0)
for f in range(300):
    e.hold(KEY.DOWN, 1)
    if e.hits():
        break
e.set_keys(0)
rows = []
for k in range(300):
    e.run(1)
    o = e.oam()
    ents = []
    for i in range(128):
        a0, a1, a2 = struct.unpack_from('<HHH', o, i * 8)
        if (a0 >> 8) & 3 == 2:
            continue
        if (a0 & 0xFF) == 128 or (a0 & 0xFF) in range(96, 160) and (a0 >> 8) & 1:
            ents.append({'i': i, 'x': a1 & 0x1FF, 'y': a0 & 0xFF, 'a0': a0, 'a1': a1, 'a2': a2,
                         'mat': (a1 >> 9) & 31 if (a0 >> 8) & 1 else None})
    mats = {}
    for en in ents:
        m = en['mat']
        if m is not None and m not in mats:
            mats[m] = [struct.unpack_from('<h', o, (m * 4 + j) * 8 + 6)[0] for j in range(4)]
    rows.append({'k': k, 'frame': e.frame, 'counter': e.r32(0x03000F94), 'oam': ents, 'mats': {str(a): b for a, b in mats.items()}})
    Image.frombytes('RGB', (240, 160), e.pixels()).save(OUT / f'f{k:03d}.png')
    if k == 2:
        # the surface: 8 OBJ blocks (7 x 32x16 + 1 x 16x16), 8bpp 1D tiles at the first OBJ's tile number
        banner = [x for x in ents if x['y'] == 128]
        if banner:
            t0 = min(b['a2'] & 0x3FF for b in banner)
            v = e.read(0x06010000 + t0 * 32, 0x1E00)
            surf = bytearray(240 * 16)
            for blk in range(8):
                w = 32 if blk < 7 else 16
                tiles_w = w // 8
                for t in range(tiles_w * 2):
                    tx, ty = t % tiles_w, t // tiles_w
                    for py in range(8):
                        for px in range(8):
                            x = blk * 32 + tx * 8 + px
                            y = ty * 8 + py
                            if x < 240:
                                surf[y * 240 + x] = v[blk * 0x200 + t * 64 + py * 8 + px]
            (OUT / 'surface.bin').write_bytes(bytes(surf))
            pal = e.palram()
            (OUT / 'obj_palette.bin').write_bytes(pal[0x200:0x400])
(OUT / 'frames.json').write_text(json.dumps(rows))
print('captured', len(rows), 'frames; banner OBJs at k=0:', rows[0]['oam'][:3], rows[0]['mats'])
alive = [r['k'] for r in rows if any(x['y'] == 128 for x in r['oam'])]
print('banner visible k', alive[:3], '...', alive[-3:], len(alive))
