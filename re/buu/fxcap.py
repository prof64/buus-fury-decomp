"""fxcap.py -- identify which effect/projectile frame records the game is drawing (OAM -> ROM).

Effect/projectile OBJs use RAW 8bpp tiles copied from ROM to a VRAM slot, so the tile bytes behind every OAM entry
can be matched against the gfx of the frame records exported as `fx_*` entities in data/buu/sprites.json. That
gives, per frame, the exact record (and so the effect id + record index), the OBJ's screen position (-> world
position via the camera), flips, OBJ mode (semi-transparency) and OAM order (draw priority).

    from fxcap import FxIndex, capture
    ix = FxIndex()
    objs = capture(e, ix)       # after e.run(n): list of dicts for the frame just drawn

Unmatched 8bpp OBJs are searched in the whole ROM (raw gfx that no record run covers -> 'rom_gfx').
"""
import json
import struct
from common import *
from wf_sprites_shim import OBJ_SIZE

CAMERA = 0x030019BC


class FxIndex:
    def __init__(self):
        d = json.loads((DATA / 'sprites.json').read_text())
        self.by_bytes = {}
        self.rec = {}
        for ent in d['entities']:
            if not ent['id'].startswith('fx_') and ent['id'] not in ('buu_halo',) and not ent['id'].startswith('buu_shadow'):
                continue
            steps = ent['animations']['default']['directions']['any'] if 'default' in ent['animations'] else []
            for i, s in enumerate(steps):
                a = int(s['rom'], 16)
                if not ent['id'].startswith('fx_'):
                    continue
                ox, oy, w, h, a0, a1, gp = struct.unpack_from('<bbBBHHI', rom(), ra(a))
                W, H = OBJ_SIZE[(a0 >> 14, a1 >> 14)]
                n = W * H
                g = rom()[ra(gp):ra(gp) + n]
                info = dict(fx=ent['id'], idx=i, rec=a, ox=ox, oy=oy, W=W, H=H, gfx=gp, a1=a1)
                self.rec[a] = info
                self.by_bytes.setdefault(g, []).append(info)
        self.romcache = {}

    def find_rom(self, g):
        if g in self.romcache:
            return self.romcache[g]
        if not any(g):
            self.romcache[g] = None
            return None
        i = rom().find(g)
        r = (0x08000000 + i) if i >= 0 else None
        self.romcache[g] = r
        return r


def camera(e):
    s = lambda v: v - (1 << 32) if v & 0x80000000 else v
    return s(e.r32(CAMERA)), s(e.r32(CAMERA + 4))


def capture(e, ix, unknown=True):
    oam = e.oam()
    vram = e.vram()
    cx, cy = camera(e)
    out = []
    for i in range(128):
        a0, a1, a2 = struct.unpack_from('<HHH', oam, i * 8)
        aff = bool(a0 & 0x100)
        if not aff and a0 & 0x200:
            continue
        sh, sz = a0 >> 14, a1 >> 14
        if sh == 3:
            continue
        W, H = OBJ_SIZE[(sh, sz)]
        bpp8 = bool(a0 & 0x2000)
        tile = a2 & 0x3FF
        x = a1 & 0x1FF
        y = a0 & 0xFF
        if x >= 240 + 64:
            x -= 512
        if y >= 160 + 64:
            y -= 256
        o = dict(oam=i, x=x, y=y, W=W, H=H, hflip=bool(a1 & 0x1000) and not aff, vflip=bool(a1 & 0x2000) and not aff,
                 mode=(a0 >> 10) & 3, prio=(a2 >> 10) & 3, affine=aff, dbl=aff and bool(a0 & 0x200),
                 bpp=8 if bpp8 else 4, pal=a2 >> 12, tile=tile)
        if aff:
            o['affine_idx'] = (a1 >> 9) & 31
        if bpp8:
            n = W * H
            g = vram[0x10000 + tile * 32:0x10000 + tile * 32 + n]
            m = ix.by_bytes.get(g)
            if m:
                fl = (a1 & 0x3000) if not aff else None
                m = sorted(m, key=lambda r: (fl is not None and (r['a1'] & 0x3000) != fl))
                o['fx'] = [(r['fx'], r['idx'], hex(r['rec'])) for r in m]
                r = m[0]
                # entity position = OBJ top-left + (ox, oy); flipped OBJs mirror the origin
                o['wx'] = x + r['ox'] + cx
                o['wy'] = y + r['oy'] + cy
            elif unknown:
                o['rom_gfx'] = ix.find_rom(g)
        out.append(o)
    return out


def affine(e, k):
    oam = e.oam()
    vals = [struct.unpack_from('<h', oam, (k * 4 + j) * 8 + 6)[0] for j in range(4)]
    return vals   # pa, pb, pc, pd (8.8)
