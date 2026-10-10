"""Save a gridded preview of a map composite to /tmp/map_<idx>.png (grid every 32 px, labels every 64) for picking arena rects.
usage: .venv/bin/python tools/battle/map_preview.py IDX [scale]"""
import os, sys
from PIL import Image, ImageDraw
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
idx = int(sys.argv[1])
sc = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
im = Image.open(os.path.join(ROOT, f'site/public/assets/log2/maps/map_{idx:03d}/composite.png')).convert('RGB')
d = ImageDraw.Draw(im)
for x in range(0, im.width, 32):
    d.line([(x, 0), (x, im.height)], fill=(255, 0, 255) if x % 64 == 0 else (120, 0, 120))
    if x % 64 == 0:
        d.text((x + 1, 1), str(x), fill=(255, 255, 0))
for y in range(0, im.height, 32):
    d.line([(0, y), (im.width, y)], fill=(255, 0, 255) if y % 64 == 0 else (120, 0, 120))
    if y % 64 == 0:
        d.text((1, y + 1), str(y), fill=(255, 255, 0))
if sc != 1.0:
    im = im.resize((int(im.width * sc), int(im.height * sc)))
im.save(f'/tmp/map_{idx}.png')
print(im.size)
