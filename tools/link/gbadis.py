"""gbadis.py ADDR LEN: Thumb disassembly of the ROM (roms/buu.gba) with pc-relative literals resolved; undecodable
halfwords are printed as .hword and skipped (data pools)."""
import capstone, struct, sys
from pathlib import Path
r = (Path(__file__).resolve().parents[2] / 'roms' / 'buu.gba').read_bytes()
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
s = int(sys.argv[1], 16)
n = int(sys.argv[2], 16)
a = s
while a < s + n:
    got = False
    for i in md.disasm(r[a - 0x08000000:s - 0x08000000 + n], a):
        got = True
        extra = ''
        if i.mnemonic == 'ldr' and 'pc' in i.op_str:
            off = int(i.op_str.split('#')[1].rstrip(']'), 16)
            p = ((i.address + 4) & ~3) + off
            extra = '  ; =%#x' % struct.unpack_from('<I', r, p - 0x08000000)[0]
        print(hex(i.address), i.mnemonic, i.op_str, extra)
        a = i.address + i.size
    if a < s + n:
        print(hex(a), '.hword', hex(struct.unpack_from('<H', r, a - 0x08000000)[0]))
        a += 2
