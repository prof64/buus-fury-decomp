"""Dump the methods of the cutscene command classes created by natives 0x3b..0x57 into
work/buu/script/nat_a_cmds.txt (vtable entries are s32 offsets relative to the vtable base)."""
import subprocess
import sys
from common import *

VTS = {
    0x3b: 0x0804c7d8, 0x3c: 0x0804d818, 0x3d: 0x0804e0e8, 0x3e: 0x0804c414, 0x3f: 0x0804b2b4, 0x40: 0x0804b2b4,
    0x41: 0x0804dce0, 0x42: 0x0804dcf4, 0x43: 0x0804b918, 0x45: 0x0804ae5c, 0x46: 0x0804c428, 0x47: 0x0804ae70,
    0x48: 0x0804c43c, 0x49: 0x0804ae84, 0x4a: 0x0804ab58, 0x4b: 0x0804c450, 0x4c: 0x0804c800, 0x4d: 0x0804ae9c,
    0x4e: 0x0804b2c8, 0x4f: 0x0804dea0, 0x51: 0x0804dd1c, 0x53: 0x0804dec8, 0x54: 0x0804c814, 0x55: 0x0804bf14,
    0x56: 0x0804dd30, 0x57: 0x0804c828, 'wait_all': 0x0804c0c4, 'group': 0x0804deec,
}
sys.argv = ['x']
from script_nat_a_fn import fn  # noqa: E402

out = []
for k, v in VTS.items():
    out.append(f'##### native {k if isinstance(k, str) else hex(k)} vtable {v:08x}')
    for i in range(6):
        t = (v + s32(v + 4 * i)) & 0xFFFFFFFF
        out.append(f'  +{4*i:02x} -> {t:08x}')
    for i in (1, 2, 3):
        t = (v + s32(v + 4 * i)) & 0xFFFFFFFF
        if 0x08000000 <= t < 0x08800000 and (t & ~1) not in (0x0804febc, 0x0804fb94, 0x0804ed14):
            out.append(f'--- method +{4*i:02x} {t:08x}')
            out += fn(t, 120)
(WORK / 'script' / 'nat_a_cmds.txt').write_text('\n'.join(out))
