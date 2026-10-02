"""Common exporter: turns normalized entities (with rendered frames) into PNGs + sheet + JSON records."""
import hashlib
import json
from pathlib import Path
from PIL import Image
from gba import ASSETS, DATA
from render import trim, pack

DIRS = ['down', 'up', 'right', 'left']


def _key(im, ox, oy):
    return hashlib.md5(im.tobytes() + bytes(f'{im.size}{ox},{oy}', 'ascii')).hexdigest()


class EntityBuilder:
    """Collects frames for one entity, dedupes (incl. mirrored duplicates), writes outputs."""

    def __init__(self, game, slug, subdir='sprites'):
        self.game = game
        self.slug = slug
        self.subdir = subdir
        self.frames = []        # (img, ox, oy, rom)
        self.index = {}         # key -> frame idx
        self.mirror = {}        # key of mirrored -> frame idx

    def add(self, im, ox, oy, rom=None):
        """Add a composed frame image (origin at ox,oy). Returns (frame_index, flip)."""
        im, ox, oy = trim(im, ox, oy)
        k = _key(im, ox, oy)
        if k in self.index:
            return self.index[k], False
        if k in self.mirror:
            return self.mirror[k], True
        i = len(self.frames)
        self.frames.append((im, ox, oy, rom))
        self.index[k] = i
        m = im.transpose(Image.FLIP_LEFT_RIGHT)
        # mirrored around the origin: new origin x = width - ox
        self.mirror.setdefault(_key(m, m.width - ox, oy), i)
        return i, False

    def write(self, write_frames=True):
        out = ASSETS / self.game / self.subdir / self.slug
        out.mkdir(parents=True, exist_ok=True)
        imgs = [f[0] for f in self.frames]
        sheet, pos = pack(imgs, max_w=max(256, min(1024, int((sum(i.width * i.height for i in imgs) ** 0.5) * 1.3) + 1)))
        sheet.save(out / 'sheet.png')
        recs = []
        for i, (im, ox, oy, rom) in enumerate(self.frames):
            if write_frames:
                im.save(out / f'{i:03d}.png')
            recs.append({'x': pos[i][0], 'y': pos[i][1], 'w': im.width, 'h': im.height,
                         'anchor': {'x': ox, 'y': oy}, 'rom': rom,
                         'png': f'assets/{self.game}/{self.subdir}/{self.slug}/{i:03d}.png' if write_frames else None})
        return f'assets/{self.game}/{self.subdir}/{self.slug}/sheet.png', recs


def hx(v):
    return None if v is None else f'0x{v:08X}'


def write_json(game, name, obj):
    p = DATA / game / name
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(obj, separators=(',', ':')))
    return p
