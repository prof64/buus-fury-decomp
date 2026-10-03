#!/usr/bin/env python3
"""Draw the app icon (generic art: a glowing energy ball with rays on a dark tile; no game art) as a 1024 px PNG.

usage: make_icon.py OUT.png
"""
import math
import sys

from PIL import Image, ImageDraw, ImageFilter

S = 1024
out = sys.argv[1] if len(sys.argv) > 1 else "icon.png"

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# rounded tile with a vertical gradient (macOS icon grid: ~100 px margin)
tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
grad = Image.new("RGBA", (S, S))
gd = ImageDraw.Draw(grad)
for y in range(S):
    t = y / S
    gd.line([(0, y), (S, y)], fill=(int(40 + 30 * t), int(18 + 10 * t), int(70 - 30 * t), 255))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([100, 100, S - 100, S - 100], radius=185, fill=255)
tile.paste(grad, (0, 0), mask)
img = Image.alpha_composite(img, tile)

cx, cy = S // 2, S // 2 + 10

# rays
rays = Image.new("RGBA", (S, S), (0, 0, 0, 0))
rd = ImageDraw.Draw(rays)
for k in range(12):
    a = k * math.pi / 6 + math.pi / 12
    r0, r1, w = 150, 360 if k % 2 == 0 else 290, 0.09
    pts = [
        (cx + r0 * math.cos(a - w), cy + r0 * math.sin(a - w)),
        (cx + r1 * math.cos(a), cy + r1 * math.sin(a)),
        (cx + r0 * math.cos(a + w), cy + r0 * math.sin(a + w)),
    ]
    rd.polygon(pts, fill=(255, 190, 60, 200))
rays = rays.filter(ImageFilter.GaussianBlur(3))
rays.putalpha(Image.composite(rays.getchannel("A"), Image.new("L", (S, S), 0), mask))
img = Image.alpha_composite(img, rays)

# glow + core
glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(glow).ellipse([cx - 230, cy - 230, cx + 230, cy + 230], fill=(255, 140, 30, 170))
glow = glow.filter(ImageFilter.GaussianBlur(60))
glow.putalpha(Image.composite(glow.getchannel("A"), Image.new("L", (S, S), 0), mask))
img = Image.alpha_composite(img, glow)
core = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(core)
for r, c in [(170, (255, 150, 40, 255)), (130, (255, 200, 80, 255)), (85, (255, 240, 180, 255)), (45, (255, 255, 255, 255))]:
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=c)
core = core.filter(ImageFilter.GaussianBlur(6))
img = Image.alpha_composite(img, core)

img.save(out)
print(out)
