"""dis.py start end [arm] -- linear Thumb (or ARM) disassembly of ROM range, skipping undecodable halfwords.
Literal-pool loads are annotated with their value."""
import sys
import struct
import capstone
from pathlib import Path

ROM = (Path(__file__).resolve().parents[4] / 'roms' / 'buu.gba').read_bytes()
start = int(sys.argv[1], 16)
end = int(sys.argv[2], 16)
arm = len(sys.argv) > 3 and sys.argv[3] == 'arm'
base = int(sys.argv[4], 16) if len(sys.argv) > 4 else start   # display address (e.g. IWRAM copy)
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_ARM if arm else capstone.CS_MODE_THUMB)
a = start
step = 4 if arm else 2
while a < end:
    o = a - 0x08000000
    ins = next(md.disasm(ROM[o:o + 4], a - start + base), None)
    if ins is None:
        print(f'{a - start + base:08x}: .hword {struct.unpack_from("<H", ROM, o)[0]:04x}')
        a += step
        continue
    s = f'{ins.address:08x}: {ins.mnemonic:7s} {ins.op_str}'
    if ins.mnemonic.startswith('ldr') and '[pc' in ins.op_str and '#' in ins.op_str:
        try:
            imm = int(ins.op_str.split('#')[1].rstrip(']'), 0)
            la = (((ins.address + 4) & ~3) if not arm else ins.address + 8) + imm
            la = la - base + start
            s += f'   ; ={struct.unpack_from("<I", ROM, la - 0x08000000)[0]:#x}'
        except Exception:
            pass
    print(s)
    a += ins.size
