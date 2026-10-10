"""romdis.py ADDR [N]  -- Thumb disassembly of the ROM (roms/buu.gba) from ADDR (hex), N instructions (default 60);
literal-pool loads are resolved"""
import struct
import sys
from pathlib import Path

import capstone

ROM = (Path(__file__).resolve().parents[2] / 'roms' / 'buu.gba').read_bytes()
a = int(sys.argv[1], 16) & ~1
n = int(sys.argv[2]) if len(sys.argv) > 2 else 60
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
off = a - 0x08000000
for k, ins in enumerate(md.disasm(ROM[off:off + n * 4], a)):
    if k >= n:
        break
    extra = ''
    if ins.mnemonic == 'ldr' and '[pc,' in ins.op_str:
        imm = int(ins.op_str.split('#')[-1].rstrip(']'), 16)
        lit = ((ins.address + 4) & ~3) + imm
        extra = f'   ; = 0x{struct.unpack_from("<I", ROM, lit - 0x08000000)[0]:08x}'
    print(f'{ins.address:08x}  {ins.mnemonic:6s} {ins.op_str}{extra}')
