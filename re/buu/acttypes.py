from common import *
T = 0x08711274
for i in range(260):
    a = T + 28 * i
    w = struct.unpack_from('<7I', rom(), ra(a))
    print(f'{i:3d} 0x{i:02x} {a:08x} ' + ' '.join(f'{x:08x}' for x in w))
