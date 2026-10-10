"""LoG1 sprite format (uncompressed 4bpp strips, 16-colour palettes).

Sprite definitions: u32[86] at 0x085C0C80 (+ projectile defs referenced from code, see EXTRA_DEFS),
0x2C bytes each:
  +0x00 u16, u16          unknown (16, 8; 32 for one boss)
  +0x04 4 x {s16 x0, y0, x1, y1}  body rectangle per facing (down, up, left, right), relative to the
                                   entity position (top-left of its 16x8 foot box)
  +0x24 u32 palette (16 colours BGR555)
  +0x28 u32 animation table: rows of 4 u32 strip pointers (one per facing); row index == entity state
        (names at 0x08010F08: Standing, Walking, Death, Melee1, Melee2, Ki, Excited, Sad, Transform,
        SSaiyaJin, Monkey, Monster, Raditz, Bubbles, Flying, Bored, Mallet). Row count is bounded by
        the next table in ROM.
Sprite (actor) names: u32[109] at 0x080109F0 -> ASCII developer labels; u8[109] at 0x0815A709 maps
sprite id -> def index.  (Tables from re/log1/romdata.py, verified here.)
Strip record (16 bytes):
  +0 s16 dx, s16 dy (top-left of OBJ relative to entity pos), u16 w, u16 h
  +8 u8 tiles per frame, u8 flags (bit3 = hflip), u8 OAM size, u8 OAM shape
  +12 u32 gfx: frames stored back-to-back, frame n at gfx + n*tiles*32 (DMA'd to OBJ VRAM by 0x0800B0C4)
Frame count per strip is not stored (state code decides); inferred from the gap to the next
strip/gfx block in ROM.
"""
import struct
import bisect
from gba import Rom
from wf_sprites import OBJ_SIZE

DEF_TABLE = 0x085C0C80
DEF_COUNT = 86
EXTRA_DEFS = {0x086B3AA8: 'ki_blast', 0x086B3AD4: 'ki_blast_medium', 0x086B3C50: 'ki_blast_small',
              0x086B376C: 'kamehameha', 0x086B3FFC: 'solar_flare', 0x086B3400: 'vegeta_ki_blast'}
NAME_PTRS = 0x080109F0
SPRITE_TO_DEF = 0x0815A709
ANIM_NAMES = ['Standing', 'Walking', 'Death', 'Melee1', 'Melee2', 'Ki', 'Excited', 'Sad', 'Transform',
              'SSaiyaJin', 'Monkey', 'Monster', 'Raditz', 'Bubbles', 'Flying', 'Bored', 'Mallet']


class Log1Sprites:
    def __init__(self):
        self.R = Rom('log1')
        self.game = 'log1'

    def strip(self, p):
        dx, dy, w, h, nt, fl, sz, sh, gp = struct.unpack('<hhHHBBBBI', self.R.bytes(p, 16))
        W, H = OBJ_SIZE[(sh, sz)]
        return {'addr': p, 'dx': dx, 'dy': dy, 'w': w, 'h': h, 'ntiles': nt, 'flags': fl,
                'hflip': bool(fl & 8), 'shape': sh, 'size': sz, 'W': W, 'H': H, 'gfx': gp}

    def valid_strip(self, p):
        if not self.R.isptr(p):
            return False
        dx, dy, w, h, nt, fl, sz, sh, gp = struct.unpack('<hhHHBBBBI', self.R.bytes(p, 16))
        return (sh, sz) in OBJ_SIZE and OBJ_SIZE[(sh, sz)] == (w, h) and nt * 64 == w * h and self.R.isptr(gp)

    def cstr(self, a):
        o = a & 0x1FFFFFF
        return self.R.d[o:self.R.d.index(b'\0', o)].decode('latin1')

    def all_defs(self):
        R = self.R
        ptrs = [R.u32(DEF_TABLE + 4 * i) for i in range(DEF_COUNT)] + list(EXTRA_DEFS)
        names = {}
        for sid in range(109):
            di = R.u8(SPRITE_TO_DEF + sid)
            names.setdefault(di, []).append(self.cstr(R.u32(NAME_PTRS + 4 * sid)))
        tables = sorted(set(R.u32(p + 0x28) for p in ptrs))
        defs = []
        strips = set()
        for i, dp in enumerate(ptrs):
            a, b = R.u16(dp), R.u16(dp + 2)
            boxes = [struct.unpack('<4h', R.bytes(dp + 4 + 8 * k, 8)) for k in range(4)]
            pal, an = R.u32(dp + 0x24), R.u32(dp + 0x28)
            j = bisect.bisect_right(tables, an)
            nrows = min(24, (tables[j] - an) // 16) if j < len(tables) else 19
            if not R.isptr(an) or not R.isptr(pal):
                nrows = 0
            anims = []
            for ai in range(nrows):
                ps = [R.u32(an + 16 * ai + 4 * d) for d in range(4)]
                if not any(ps) or not all(p == 0 or self.valid_strip(p) for p in ps):
                    anims.append(None)
                    continue
                anims.append({'strips': ps})
                strips.update(p for p in ps if p)
            labels = names.get(i, []) if i < DEF_COUNT else [EXTRA_DEFS[dp]]
            defs.append({'index': i, 'id': i, 'addr': dp, 'palette_addr': pal, 'anim_table': an,
                         'labels': labels, 'header': {'word0': a, 'word2': b}, 'boxes': boxes, 'anims': anims})
        # frame counts from ROM layout
        bounds = set()
        for p in strips:
            s = self.strip(p)
            bounds.add(p)
            bounds.add(s['gfx'])
        bounds = sorted(bounds)
        self._nf = {}
        for p in strips:
            s = self.strip(p)
            fb = s['ntiles'] * 32
            j = bisect.bisect_right(bounds, s['gfx'])
            nxt = bounds[j] if j < len(bounds) else s['gfx'] + fb
            self._nf[p] = max(1, min(16, (nxt - s['gfx']) // fb))
        for d in defs:
            for a in d['anims']:
                if a:
                    a['dirs'] = [[(p, k) for k in range(self._nf[p])] if p else [] for p in a['strips']]
                    a['nframes'] = max(len(x) for x in a['dirs'])
        return defs

    def frame_gfx(self, p, k):
        s = self.strip(p)
        n = s['ntiles'] * 32
        return s, self.R.bytes(s['gfx'] + k * n, n)
