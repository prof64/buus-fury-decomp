import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
e = emu('buu_menus.ss')
obj = 0x0203dee4
p = e.r32(obj + 0x10) & ~1
p0 = p - 0x200
b = e.read(p0, 0x600)
s = ''
for i in range(0, len(b), 2):
    c = struct.unpack_from('<H', b, i)[0]
    if 0x20 <= c < 0x7f:
        s += chr(c)
    elif c == 10:
        s += '\\n'
    else:
        s += '{%x}' % c
    if p0 + i == p:
        s += '<<HERE>>'
print(hex(p), s)
