"""world_objs.py STATE.ss -- every object of the object world (0x03001C4C): vtable, box (+0x14 .. +0x20)"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401
from route import start_emu, status  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
print(status(e))
n = e.r32(0x03001C4C + 4)
sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
for k in range(min(n, 255)):
    o = e.r32(0x03001C4C + 8 + 4 * k)
    print(hex(o), hex(e.r32(o)), [sv(e.r32(o + 0x14 + 4 * j)) for j in range(4)])
