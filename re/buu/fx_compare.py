"""fx_compare.py out_name sim_png sim_json emu_png emu_json emu_frame [w h scale]
Side-by-side crop (emulator left, sim right) centred on the player -> work/compare/fx/<out_name>.png"""
import sys
import json
from PIL import Image, ImageDraw
from common import *

a = sys.argv
name, sim_png, sim_json, emu_png, emu_json, ef = a[1], a[2], a[3], a[4], a[5], int(a[6])
W = int(a[7]) if len(a) > 7 else 200
H = int(a[8]) if len(a) > 8 else 120
S = int(a[9]) if len(a) > 9 else 3
sm = json.load(open(sim_json))
sim = Image.open(sim_png).convert('RGB')
emu = Image.open(emu_png).convert('RGB')
emu_scale = emu.width // 240
emu = emu.resize((240, 160), Image.NEAREST)
L = json.load(open(emu_json))
fr = next(x for x in L if x['f'] == ef)
ex, ey = fr['px'] - fr['cam'][0], fr['py'] - fr['cam'][1]


def crop(im, cx, cy):
    out = Image.new('RGB', (W, H), (0, 0, 0))
    x0, y0 = int(cx) - W // 2 + 8, int(cy) - H // 2
    x0 = max(0, min(im.width - W, x0))
    y0 = max(0, min(im.height - H, y0))
    out.paste(im.crop((x0, y0, x0 + W, y0 + H)), (0, 0))
    return out.resize((W * S, H * S), Image.NEAREST)


A = crop(emu, ex, ey)
B = crop(sim, sm['sx'], sm['sy'])
out = Image.new('RGB', (W * S * 2 + 8, H * S + 20), (30, 30, 40))
out.paste(A, (0, 20))
out.paste(B, (W * S + 8, 20))
d = ImageDraw.Draw(out)
d.text((4, 4), f'emulator f{ef}', fill=(255, 255, 0))
d.text((W * S + 12, 4), f'sim {Path(sim_png).stem}', fill=(255, 255, 0))
p = ROOT / 'work' / 'compare' / 'fx' / f'{name}.png'
out.save(p)
print(p)
