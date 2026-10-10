"""evprobe.py STATE.ss: the running event at a state - scene objects on the stack (words equal to a scene-table header,
followed by result ptr / state), the script context stack 0x03001034 (interpreter objects: value stack, param block),
and stack words pointing into ROM/EWRAM bytecode near the scenes' current steps."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

e = start_emu(str(Path(sys.argv[1]).resolve()))
TABLE = 0x0879B704
N = 1200
hdr = {}
for k in range(N):
    h = e.r32(TABLE + 4 * k)
    if 0x08000000 <= h < 0x0A000000:
        hdr.setdefault(h, k)
print('depth', e.r32(0x03002A5C), 'ctx depth', e.r32(0x03001034), 'cur params', hex(e.r32(0x03001030)))
for i in range(1, 1 + min(e.r32(0x03001034), 8)):
    it = e.r32(0x03001034 + 4 * i)
    sp = e.r32(it)
    print(' interp', hex(it), 'sp', sp, 'stack', [e.r32(it + 4 + 4 * j) for j in range(min(sp, 16))], 'P ptr', hex(e.r32(it + 0x58)))
lo, hi = 0x03007000, 0x03008000
for a in range(lo, hi, 4):
    v = e.r32(a)
    if v in hdr:
        st = e.r16(a + 8)
        cur = e.r32(a + 0x28)
        print('scene obj', hex(a), 'scene', hdr[v], 'state', st, 'step rec', hex(cur), 'step kind', e.r8(cur + 2) if 0x08000000 <= cur < 0x0A000000 else None)
print('stack words into ROM code / EWRAM:')
for a in range(lo, hi, 4):
    v = e.r32(a)
    if 0x08700000 <= v < 0x08800000 or 0x02000000 <= v < 0x02040000:
        print(' ', hex(a), hex(v))
