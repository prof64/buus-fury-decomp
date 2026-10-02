"""script_text_render_find.py -- locate the text layout / renderer: Thumb code windows (0x100 bytes) that compare a
register with 0x08, 0x0A and 0x0B (the dialog control units)."""
import capstone
from common import *

md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
md.skipdata = True
R = rom()
hits = {}
for ins in md.disasm(R[0:0x50000], 0x08000000):
    if ins.mnemonic == 'cmp' and '#' in ins.op_str:
        try:
            v = int(ins.op_str.split('#')[1], 16)
        except ValueError:
            continue
        if v in (8, 0xa, 0xb, 0x25):
            hits.setdefault(ins.address & ~0xFF, set()).add(v)
for a in sorted(hits):
    s = hits[a] | hits.get(a + 0x100, set())
    if {8, 0xa, 0xb} <= s:
        print(hex(a), sorted(s))
