"""Code-driven effect / projectile / pickup sprites (LoG2, Buu).

These are not reached from the sprite-definition table. Each frame is a 12-byte record
  s8 ox, s8 oy  (origin inside the OBJ: OBJ top-left = position - (ox, oy))
  u8 w, u8 h, u16 attr0, u16 attr1, u32 gfx (RAW, uncompressed tiles)
and runs of consecutive records form one effect. Records are located by scanning the ROM for that
shape (OBJ size consistent with w/h, raw gfx pointer); runs are split where the gfx block changes
discontinuously.
"""
import struct
import numpy as np
from gba import Rom
from wf_sprites import OBJ_SIZE


def scan(game, known_frames):
    R = Rom(game)
    d = R.d
    a = np.frombuffer(d, dtype='<u4')
    recs = []
    cand = np.nonzero((a >= 0x8000000) & (a < 0x8000000 + len(d)))[0]
    for i in cand:
        i = int(i)
        if i < 2:
            continue
        o = (i - 2) * 4
        gp = int(a[i])
        ox, oy, w, h, a0, a1 = struct.unpack_from('<bbBBHH', d, o)
        sh, sz = a0 >> 14, a1 >> 14
        if (sh, sz) not in OBJ_SIZE or (a0 & 0x1F00 & ~0x2000) or (a1 & 0x0FFF):
            continue
        W, H = OBJ_SIZE[(sh, sz)]
        if (w, h) != (W, H) or 0x8000000 + o in known_frames:
            continue
        if R.u32(gp) in (1, 2) and R.u32(gp + 4) in (W * H, W * H // 2):
            continue
        if gp & 3 or o < 0x100000 or gp < 0x8100000:
            continue  # records and pixel data live in the data area, not among code
        if abs(ox) > W + 8 or abs(oy) > H + 24:
            continue
        recs.append({'addr': 0x8000000 + o, 'ox': ox, 'oy': oy, 'W': W, 'H': H, 'attr0': a0, 'attr1': a1,
                     'gfx': gp, 'bpp': 8 if a0 & 0x2000 else 4,
                     'hflip': bool(a1 & 0x1000), 'vflip': bool(a1 & 0x2000)})
    runs = []
    for r in recs:
        if runs and r['addr'] - runs[-1][-1]['addr'] == 12:
            runs[-1].append(r)
        else:
            runs.append([r])
    return runs


def raw_gfx(R, r):
    n = r['W'] * r['H'] * r['bpp'] // 8
    return R.bytes(r['gfx'], n)
