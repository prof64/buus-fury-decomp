"""Capture the LoG1 HUD move icons (Ki Blast / Kamehameha / Solar Flare) and a flying + Kamehameha reference
screenshot for the battle simulator.

The HUD (drawn at screen 8,8) shows the selected ki move (controller+5) in its right box (8x8 at HUD 64,4).
We force each selection with all moves learned (0x0300004D bits 1,2), render a frame and crop the box.
Writes site/public/assets/log1/battle/icon_<n>.png (10x10 incl. the dark border).

Uses the save state work/states/log1_wolf.ss when it exists; on a fresh checkout (no states) it warps to map 2 from
boot instead (tools/extract/emu_states.py). `--fresh` forces the warp, `--out DIR` writes elsewhere.

usage: .venv/bin/python tools/battle/log1_icons.py [--fresh] [--out DIR]
"""
import os, sys
from PIL import Image
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/log1'))
from common import STATES, emu  # noqa: E402

OUT = os.path.join(ROOT, 'site/public/assets/log1/battle')
if '--out' in sys.argv:
    OUT = sys.argv[sys.argv.index('--out') + 1]
os.makedirs(OUT, exist_ok=True)
if (STATES / 'log1_wolf.ss').exists() and '--fresh' not in sys.argv:
    e = emu('log1_wolf.ss')
else:
    sys.path.insert(0, os.path.join(ROOT, 'tools/extract'))
    import emu_states  # noqa: E402
    e = emu_states.log1_warp(2)   # Mt. Paozu, default entrance: in-game with the HUD up
e.w8(0x0300004D, e.r8(0x0300004D) | 6)
for m in range(3):
    e.w8(0x030007AC + 5, m)
    e.run(3)
    img = Image.frombytes('RGB', (240, 160), e.pixels())
    img.crop((8 + 63, 8 + 3, 8 + 73, 8 + 13)).save(os.path.join(OUT, 'icon_%d.png' % m))
    if m == 0:
        img.crop((8, 8, 88, 24)).save(os.path.join(OUT, 'hud_ref.png'))
# flight HUD (feather + energy box) replaces the normal HUD while flying
e.w8(0x030007AC + 5, 0)
e.press(0x100, 1, 0)  # R: take off
e.run(20)
img = Image.frombytes('RGB', (240, 160), e.pixels())
img.crop((0, 0, 72, 36)).save(os.path.join(OUT, 'fly_hud_ref.png'))
# feather medal (16x16 at screen 6,12), background outside the circle made transparent
fe = img.crop((6, 12, 22, 28)).convert('RGBA')
px = fe.load()
for y in range(16):
    for x in range(16):
        if (x - 7.5) ** 2 + (y - 7.5) ** 2 > 8.2 ** 2:
            px[x, y] = (0, 0, 0, 0)
fe.save(os.path.join(OUT, 'feather.png'))
print('ok')
