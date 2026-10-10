"""script_nat_b_film.py SPEC name every [state] [maxframes] -- like script_nat_b_probe but films a frame strip
(every N frames, up to 12 shots, 2x zoom of the 120x80 area around the player)."""
import sys
from common import *
from script_nat_b_lib import run_code, asm
from script_nat_b_probe import parse
from PIL import Image

spec, name, every = sys.argv[1], sys.argv[2], int(sys.argv[3])
state = sys.argv[4] if len(sys.argv) > 4 else 'buu_snakeway_start.ss'
mx = int(sys.argv[5]) if len(sys.argv) > 5 else 600
e = emu(state)
e.run(3)
shots = []


def pf(e, f):
    if f % every == 0 and len(shots) < 12:
        shots.append(Image.frombytes('RGB', (240, 160), e.pixels()).crop((60, 0, 180, 100)).resize((240, 200)))


print(run_code(e, asm(parse(spec)), max_frames=mx, per_frame=pf))
W = Image.new('RGB', (240 * 6, 200 * ((len(shots) + 5) // 6)))
for i, im in enumerate(shots):
    W.paste(im, (240 * (i % 6), 200 * (i // 6)))
p = WORK / 'script' / f'film_{name}.png'
W.save(p)
print(p)
