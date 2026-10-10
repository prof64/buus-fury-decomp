"""blscan.py target [target...] -- find Thumb BL call sites to the targets and disassemble a few instructions before."""
import sys
import struct
from pathlib import Path
import capstone

ROM = (Path(__file__).resolve().parents[4] / 'roms' / 'buu.gba').read_bytes()
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
targets = {int(t, 16) & ~1 for t in sys.argv[1:]}
for o in range(0, 0x80000, 2):
    hi, lo = struct.unpack_from('<HH', ROM, o)
    if hi & 0xF800 != 0xF000 or lo & 0xF800 != 0xF800:
        continue
    off = ((hi & 0x7FF) << 12) | ((lo & 0x7FF) << 1)
    if off & 0x400000:
        off -= 0x800000
    a = 0x08000000 + o
    t = a + 4 + off
    if t in targets:
        print(f'--- call {t:08x} at {a:08x}')
        code = ROM[o - 16:o + 4]
        for ins in md.disasm(code, a - 16):
            print(f'  {ins.address:08x}: {ins.mnemonic} {ins.op_str}')
