from common import *
from strtab import S
T = 0x086b82ec
for i in range(60):
    a = T + 24 * i
    b = rom()[ra(a):ra(a) + 24]
    f0, nm, NM, ptr, ci, cj, form = struct.unpack_from('<HHIIBBH', b)
    print(f'{i:2d} {a:08x} f0={f0:04x} name={S(nm)!r:10} NAME={S(NM)!r:10} ptr={ptr:08x} ci={ci} cj={cj} form={form:x}:{S(form)!r:16} rest={b[16:].hex(" ")}')
