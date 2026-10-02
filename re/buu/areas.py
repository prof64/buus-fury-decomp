from common import *
from strtab import S
T = 0x0808e2e0
for i in range(0x1c4):
    a = T + 0x38 * i
    b = rom()[ra(a):ra(a) + 0x38]
    hw = struct.unpack_from('<28H', b)
    w = struct.unpack_from('<14I', b)
    nm = S(w[3]) if w[3] else ''
    print(f'{i:3d} {a:08x} key=({b[0]},{b[1]}) b2={b[2]} b3={b[3]} {nm!r:24} ' + ' '.join(f'{x:08x}' for x in w[1:]))
