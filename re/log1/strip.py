"""strip.py STATE OUT.png every count cmds... : run cmds then capture `count` frames every `every` frames into a grid."""
import sys
from PIL import Image
from common import *

st, outp, every, count = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
e = emu(st)
seq = []
for c in sys.argv[5:]:
    k, n = c.split(':')
    keys = 0
    if k != '0':
        for nm in k.split('+'):
            keys |= getattr(KEY, nm)
    seq += [keys] * int(n)
frames = []
total = max(len(seq), every * count)
for i in range(total):
    e.set_keys(seq[i] if i < len(seq) else 0)
    e.run(1)
    if i % every == 0 and len(frames) < count:
        frames.append(Image.frombytes('RGB', (240, 160), e.pixels()))
cols = min(6, len(frames))
rows = (len(frames) + cols - 1) // cols
im = Image.new('RGB', (240 * cols, 160 * rows))
for i, f in enumerate(frames):
    im.paste(f, ((i % cols) * 240, (i // cols) * 160))
im.save(SHOTS / outp)
