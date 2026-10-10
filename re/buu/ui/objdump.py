"""objdump.py STATEFILE OUT.png [x0 y0 x1 y1] : OBJ layer (all visible OAM) or a crop, and print 8bpp OBJ VRAM tile raw bytes for HUD"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/tools/extract')
from common import *
from ui_export import render_oam
e = emu(sys.argv[1])
im, n = render_oam(e)
if len(sys.argv) > 3:
    im = im.crop(tuple(int(v) for v in sys.argv[3:7]))
im.save(sys.argv[2])
print(n, 'objs')
