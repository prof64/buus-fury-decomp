"""pts.py STATE.ss -- level / unspent stat points / stats / hp of every character record"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401  (sets the tool paths)
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
for c in range(5):
    r = 0x03001560 + 0x68 * c
    print(c, 'hp', e.r16(r + 0x16), '/', e.r16(r + 0x18), 'ep', e.r16(r + 0x1A), 'xp', e.r32(r + 0x24),
          'stats', list(e.read(r + 0x28, 4)), 'points', e.r8(r + 0x2C), 'bytes', list(e.read(r, 0x16)))
