"""cap.py -- capture menu traces from the emulator (headless, muted).

Library: Cap(state).run(keys...) records per video frame: keys, sound calls, screen RGB, and optional RAM probes.
Key program syntax: 'START:1' = hold START for 1 frame; 'N:10' / '-:10' = nothing for 10 frames; 'A+UP:2'.
"""
import sys, json, hashlib, zlib
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import emu, KEY, ROOT  # noqa
import numpy as np

PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG = 0x0803E378, 0x0803E302, 0x0803E2EC, 0x0803E32C, 0x0803F110
SFX_TABLE, SONG_TABLE = 0x083192A8, 0x083BB78C
OUT = ROOT / 'work' / 'buu' / 'menu'
OUT.mkdir(parents=True, exist_ok=True)


def keybits(s):
    if s in ('N', '-', ''):
        return 0
    v = 0
    for k in s.split('+'):
        v |= getattr(KEY, k.upper())
    return v


def program(steps):
    out = []
    for st in steps:
        if ':' in st:
            k, n = st.rsplit(':', 1)
            n = int(n)
        else:
            k, n = st, 1
        out += [keybits(k)] * n
    return out


class Cap:
    def __init__(self, state=None, e=None):
        self.e = e or emu(state)
        self.bps = [self.e.breakpoint(a) for a in (PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG)]
        self.e.hits()
        self.frames = []
        self.screens = []

    def snd(self):
        out = []
        for h in self.e.hits():
            if h['kind'] != 'break':
                continue
            a, r = h['addr'] & ~1, h['regs']
            if a == PLAY_SFX:
                out.append(['play', (r[1] - SFX_TABLE) // 12, r[2] & 0xFF])
            elif a == STOP_SFX:
                out.append(['stop', (r[1] - SFX_TABLE) // 12])
            elif a == STOP_ALL:
                out.append(['stopall'])
            elif a == STOP_MATCH:
                out.append(['stopmatch', (r[1] - SFX_TABLE) // 12])
            elif a == PLAY_SONG:
                out.append(['song', (r[1] - SONG_TABLE) // 20 if SONG_TABLE <= r[1] < SONG_TABLE + 53 * 20 else -1])
        return out

    def step(self, keys, probe=None, keep=True):
        self.e.set_keys(keys)
        self.e.run(1)
        px = self.e.pixels()
        rgb = np.frombuffer(px, dtype=np.uint8).reshape(160, 240, 3).copy()
        row = dict(keys=keys, snd=self.snd(), h=hashlib.sha1(rgb.tobytes()).hexdigest()[:16])
        if probe:
            row.update(probe(self.e))
        self.frames.append(row)
        if keep:
            self.screens.append(rgb)
        return row

    def run(self, steps, probe=None):
        for k in program(steps):
            self.step(k, probe)

    def save(self, name, meta=None):
        d = OUT / name
        d.mkdir(parents=True, exist_ok=True)
        np.save(d / 'screens.npy', np.stack(self.screens) if self.screens else np.zeros((0, 160, 240, 3), np.uint8))
        (d / 'frames.json').write_text(json.dumps(dict(meta=meta or {}, frames=self.frames)))
        return d

    def png(self, i, path):
        from PIL import Image
        Image.fromarray(self.screens[i]).save(path)


def contact(screens, idx, path, cols=6):
    from PIL import Image
    n = len(idx)
    rows = (n + cols - 1) // cols
    im = Image.new('RGB', (cols * 242, rows * 172), (40, 0, 40))
    from PIL import ImageDraw
    dr = ImageDraw.Draw(im)
    for j, i in enumerate(idx):
        x, y = (j % cols) * 242, (j // cols) * 172
        im.paste(Image.fromarray(screens[i]), (x, y))
        dr.text((x + 2, y + 161), str(i), fill=(255, 255, 0))
    im.save(path)
