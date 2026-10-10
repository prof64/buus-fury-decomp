"""vramowner.py STATE.ss START...: which object holds the sprite block starting at START (allocator units; handle =
start << 1): world-list objects and every EWRAM object whose halfword equals the handle (+0xC / +0x16 ...)"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

e = start_emu(str(Path(sys.argv[1]).resolve()))
c = e.r32(0x03001C4C + 4)
objs = [e.r32(0x03001C4C + 8 + 4 * k) for k in range(min(c, 255))]
for s in sys.argv[2:]:
    h = int(s) << 1
    hits = []
    for o in objs:
        for off in range(0, 0x80, 2):
            if (e.r32((o + off) & ~3) >> (16 * (((o + off) >> 1) & 1))) & 0xFFFF == h:
                hits.append((hex(o), hex(e.r32(o)), hex(off)))
    print(s, 'handle', hex(h), hits[:6])
