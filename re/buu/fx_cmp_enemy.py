"""fx_cmp_enemy.py -- emulator side of the enemy projectile comparisons: force-spawn each enemy 70 px right of the
player (re-pinned until it fires), screenshot up to 3 frames (>= 20 apart) where effect OBJs are on screen, then
compose emulator | sim pairs into work/compare/fx/enemy_<name>_<k>.png (sim: work/compare/fx/sim/enemy_<name>_<k>.png)."""
import sys
import json
from PIL import Image, ImageDraw
from common import *
from forcespawn import spawn
from fxcap import FxIndex, capture
from ents import PLAYER

REC = 0x03001560
ENEMIES = [('gunman', 0x2c), ('annihilator', 0x01), ('bomber', 0x08), ('hessian', 0x2e), ('soldier', 0x45),
           ('mummy', 0x4d), ('kidbuu', 0x3c), ('ninja', 0x4e)]
OUT = ROOT / 'work' / 'compare' / 'fx'
ix = FxIndex()
only = sys.argv[1:]
for name, t in ENEMIES:
    if only and name not in only:
        continue
    e, o, ok = spawn(t)
    if not o:
        print(name, 'no spawn')
        continue
    p = e.r32(PLAYER)
    shots = []
    last = -100
    for f in range(1500):
        e.w16(REC + 0x16, 9999)
        e.w16(REC + 0x18, 9999)
        if f < 5:
            e.w32(o + 0x1a0, (e.r32(p + 0x1a0) + (70 << 8)) & 0xffffffff)
            e.w32(o + 0x1a4, e.r32(p + 0x1a4))
        e.run(1)
        objs = [x for x in capture(e, ix, unknown=False) if 'fx' in x]
        if objs and f - last >= 20:
            last = f
            pth = OUT / 'emu' / f'enemy_{name}_{len(shots) + 1}.png'
            pth.parent.mkdir(exist_ok=True)
            e.screenshot(str(pth), scale=1)
            shots.append(f)
            if len(shots) >= 3:
                break
    print(name, shots)
    for k in range(1, len(shots) + 1):
        sp = OUT / 'sim' / f'enemy_{name}_{k}.png'
        if not sp.exists():
            continue
        sm = json.loads(sp.with_suffix('.json').read_text())
        sim = Image.open(sp).convert('RGB')
        emu = Image.open(OUT / 'emu' / f'enemy_{name}_{k}.png').convert('RGB').resize((240, 160), Image.NEAREST)
        x0 = max(0, min(sim.width - 240, int(sm['sx']) - 100))
        y0 = max(0, min(sim.height - 160, int(sm['sy']) - 80))
        simc = sim.crop((x0, y0, x0 + 240, y0 + 160))
        out = Image.new('RGB', (240 * 3 * 2 + 8, 160 * 3 + 20), (30, 30, 40))
        out.paste(emu.resize((720, 480), Image.NEAREST), (0, 20))
        out.paste(simc.resize((720, 480), Image.NEAREST), (728, 20))
        d = ImageDraw.Draw(out)
        d.text((4, 4), f'emulator {name} f{shots[k - 1]}', fill=(255, 255, 0))
        d.text((732, 4), f'sim {name} f{sm["f"]}', fill=(255, 255, 0))
        out.save(OUT / f'enemy_{name}_{k}.png')
