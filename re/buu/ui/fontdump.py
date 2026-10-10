"""fontdump.py FONTSTRUCT... : render glyphs 0..255 of a font to out/font_<addr>.png (16 per row, 18 px cells)"""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from PIL import Image
for fa in sys.argv[1:]:
    f = int(fa, 16)
    t = u32(f)
    im = Image.new('RGB', (16 * 18, 16 * 18), (40, 40, 80))
    px = im.load()
    n = 0
    maxh = 0
    for c in range(256):
        g = u16(t + 2 * c)
        if not g:
            continue
        a = t + 2 * g
        w = u16(a)
        if w > 16:
            continue
        n += 1
        for x in range(w):
            col = u16(a + 2 + 2 * x)
            for y in range(16):
                if col >> y & 1:
                    px[(c % 16) * 18 + x, (c // 16) * 18 + y] = (255, 255, 255)
                    maxh = max(maxh, y + 1)
    im = im.resize((im.width * 3, im.height * 3), Image.NEAREST)
    im.save(f'/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui/out/font_{f:08x}.png')
    print(fa, 'T', hex(t), 'glyphs', n, 'max rows', maxh, 'field4', u32(f + 4), 'field8', hex(u32(f + 8)))
