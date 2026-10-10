"""crop.py in.png x y w h scale out.png"""
import sys
from PIL import Image
a = sys.argv
im = Image.open(a[1])
x, y, w, h, s = map(int, a[2:7])
im.crop((x, y, x + w, y + h)).resize((w * s, h * s), Image.NEAREST).save(a[7])
