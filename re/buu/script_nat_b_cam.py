"""script_nat_b_cam.py SPEC [maxframes] -- run bytecode and log the camera singleton (0x03002BD8) words that
change each frame (first 0x60 bytes), to decode camera natives."""
import sys
from common import *
from script_nat_b_lib import run_code, asm
from script_nat_b_probe import parse

CAM = int(sys.argv[3], 16) if len(sys.argv) > 3 else 0x03002BD8
spec = sys.argv[1]
mx = int(sys.argv[2]) if len(sys.argv) > 2 else 300
e = emu('buu_snakeway_start.ss')
e.run(3)
prev = [e.r32(CAM + 4 * i) for i in range(24)]
print('cam0', ' '.join(f'{v:08x}' for v in prev))


def pf(e, f):
    global prev
    cur = [e.r32(CAM + 4 * i) for i in range(24)]
    d = {f'+{4*i:02x}': (cur[i] if cur[i] < 0x80000000 else cur[i] - (1 << 32)) for i in range(24) if cur[i] != prev[i]}
    if d and (f < 12 or f % 10 == 0):
        print(f, d)
    prev = cur


print(run_code(e, asm(parse(spec)), max_frames=mx, per_frame=pf))
