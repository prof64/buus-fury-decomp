"""Sprite definition parsers for the Webfoot engine (LoG2 and Buu's Fury) and LoG1.

All parsers produce the same normalized model:

  Entity = {
    'index': table index, 'addr': def address,
    'header': {...raw fields...},
    'remap': list[256] | None    (pixel index remap applied to 8bpp pixels; palette swap)
    'boxes': [(x0,y0,x1,y1)] * 4  (per-direction body rectangle, relative to entity position)
    'anims': [ None | {'dirs': [ [frame_addr...] * 4 ], 'raw': ...} ]
  }
  Frame = {'addr', 'bbox': (dx,dy,w,h), 'objs': [ {dx,dy,w,h,attr0,attr1,gfx, bpp, pal} ]}
"""
import struct
from gba import Rom, wf_decompress

OBJ_SIZE = {(0, 0): (8, 8), (0, 1): (16, 16), (0, 2): (32, 32), (0, 3): (64, 64),
            (1, 0): (16, 8), (1, 1): (32, 8), (1, 2): (32, 16), (1, 3): (64, 32),
            (2, 0): (8, 16), (2, 1): (8, 32), (2, 2): (16, 32), (2, 3): (32, 64)}

# ROM layout constants per game
LAYOUT = {
    'log2': {'def_table': 0x83B4E90, 'def_count': 150, 'id_base': 7,
             'obj_palette': 0x81DA6C8, 'bg_palette': 0x81DA4C8},
    'buu': {'def_table': 0x86B6BD8, 'def_count': 298, 'id_base': 0,
            'obj_palette': None, 'bg_palette': None},
}


class WFSprites:
    """LoG2 / Buu sprite definition reader."""

    def __init__(self, game):
        self.game = game
        self.R = Rom(game)
        self.L = LAYOUT[game]
        self._gfx = {}

    # ---------- frames
    def obj_at(self, p):
        dx, dy, w, h, a0, a1, gp = struct.unpack('<bbBBHHI', self.R.bytes(p, 12))
        W, H = OBJ_SIZE[(a0 >> 14, (a1 >> 14))]
        return {'dx': dx, 'dy': dy, 'w': w, 'h': h, 'attr0': a0, 'attr1': a1, 'gfx': gp,
                'W': W, 'H': H, 'bpp': 8 if a0 & 0x2000 else 4,
                'hflip': bool(a1 & 0x1000), 'vflip': bool(a1 & 0x2000)}

    def valid_gfx(self, gp):
        R = self.R
        if not R.isptr(gp) or gp & 3:
            return False
        t = R.u32(gp)
        n = R.u32(gp + 4)
        return t in (0, 1, 2) and 0 < n <= 0x2000

    def frame(self, p):
        """Parse a frame at address p (format depends on game)."""
        R = self.R
        if self.game == 'log2':
            o = self.obj_at(p)
            return {'addr': p, 'bbox': (o['dx'], o['dy'], o['w'], o['h']), 'objs': [o], 'size': 12}
        n = R.u32(p)
        dx, dy, w, h = struct.unpack('<bbBB', R.bytes(p + 4, 4))
        objs = [self.obj_at(p + 8 + 12 * i) for i in range(n)]
        return {'addr': p, 'bbox': (dx, dy, w, h), 'objs': objs, 'size': 8 + 12 * n}

    def valid_frame(self, p):
        R = self.R
        if not R.isptr(p) or p & 3:
            return False
        try:
            if self.game == 'log2':
                o = self.obj_at(p)
                return self.valid_gfx(o['gfx']) and (o['attr0'] & 0x0300) == 0
            n = R.u32(p)
            if not 1 <= n <= 16:
                return False
            for i in range(n):
                o = self.obj_at(p + 8 + 12 * i)
                if not self.valid_gfx(o['gfx']) or (o['attr0'] & 0x0300):
                    return False
            return True
        except (KeyError, struct.error):
            return False

    def gfx(self, gp):
        if gp not in self._gfx:
            self._gfx[gp] = wf_decompress(self.R.d, gp & 0x1FFFFFF)[0]
        return self._gfx[gp]

    # ---------- defs
    def def_ptrs(self):
        R = self.R
        t = self.L['def_table']
        return [R.u32(t + 4 * i) for i in range(self.L['def_count'])]

    def parse_def(self, dp):
        R = self.R
        if self.game == 'log2':
            b0, b1, na, b3, w1, rp = struct.unpack('<BBBBII', R.bytes(dp, 12))
            hdr = {'flags0': b0, 'flags1': b1, 'nanims': na, 'size_class': b3, 'word4': w1}
            boxes_at = dp + 12
            anims_at = dp + 0x4C
        else:
            nm1, f1, f2, f3, rp, s0, s1, s2, s3, w = struct.unpack('<BBBBIBBBBI', R.bytes(dp, 16))
            na = nm1 + 1
            hdr = {'nanims': na, 'flags1': f1, 'flags2': f2, 'flags3': f3,
                   'shadow': [s0, s1, s2, s3], 'word12': w}
            boxes_at = dp + 16
            anims_at = dp + 0x50
        if na > 64 or (rp and not R.isptr(rp)):
            return None
        boxes = [struct.unpack('<4i', R.bytes(boxes_at + 16 * d, 16)) for d in range(4)]
        anims = []
        for a in range(na):
            if self.game == 'log2':
                lists = [R.u32(anims_at + 16 * a + 4 * d) for d in range(4)]
                if not any(lists):
                    anims.append(None)
                    continue
                anims.append({'lists': lists})
            else:
                ap = R.u32(anims_at + 4 * a)
                if not ap:
                    anims.append(None)
                    continue
                nf = R.u32(ap)
                # frame-major: entry (frame f, dir d) at ap + 4 + 16*f + 4*d  (code at 0x08010192)
                dirs = [[R.u32(ap + 4 + 16 * f + 4 * d) for f in range(nf)] for d in range(4)]
                anims.append({'addr': ap, 'nframes': nf, 'dirs': dirs})
        return {'addr': dp, 'header': hdr, 'remap': list(R.bytes(rp, 256)) if rp else None,
                'remap_addr': rp, 'boxes': boxes, 'anims': anims}

    def resolve_lists(self, defs):
        """LoG2 stores per-direction frame lists without counts. Infer counts: a list runs until
        the next known list start or the first invalid entry."""
        if self.game != 'log2':
            return
        starts = set()
        for d in defs:
            if not d:
                continue
            for a in d['anims']:
                if a:
                    starts.update(p for p in a['lists'] if p)
        for d in defs:
            if not d:
                continue
            for a in d['anims']:
                if not a:
                    continue
                dirs = []
                for p in a['lists']:
                    fr = []
                    q = p
                    while p and self.valid_frame(q) and (q == p or q not in starts) and len(fr) < 64:
                        fr.append(q)
                        q += 12
                    dirs.append(fr)
                a['dirs'] = dirs
                a['nframes'] = max(len(x) for x in dirs)

    def all_defs(self):
        defs = []
        for i, dp in enumerate(self.def_ptrs()):
            d = None
            if self.R.isptr(dp):
                try:
                    d = self.parse_def(dp)
                except struct.error:
                    d = None
                if d and self.game == 'buu' and not self._buu_def_ok(d):
                    d = None
            if d:
                d['index'] = i
                d['id'] = i + self.L['id_base']
            defs.append(d)
        self.resolve_lists(defs)
        return defs

    def _buu_def_ok(self, d):
        for a in d['anims']:
            if a is None:
                continue
            if not 0 < a['nframes'] <= 64:
                return False
            for fl in a['dirs']:
                for f in fl:
                    if f and not self.valid_frame(f):
                        return False
        return any(d['anims'])
