"""Warp to a map with a breakpoint on the palette fade routine 0x08007074 (r0,r1,r2 = target RGB, r3 = level):
print the distinct (args, caller) seen and the frames."""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/tools/extract')
import emu_states
from emu_states import buu_snakeway

orig = buu_snakeway


def patched():
    e = orig()
    patched.e = e
    e.breakpoint(0x08007074)
    return e


emu_states.buu_snakeway = patched
world, area = int(sys.argv[1]), int(sys.argv[2])
e, ok = emu_states.buu_warp(world, area)
last = None
for h in e.hits():
    r = h['regs']
    k = (r[0], r[1], r[2], r[3], r[14])
    if k != last:
        print(f'f{h["frame"]} r0={r[0]:x} r1={r[1]:x} r2={r[2]:x} r3={r[3]:x} lr={r[14]:08x}')
    last = k
