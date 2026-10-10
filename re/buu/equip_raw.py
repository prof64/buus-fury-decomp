from common import *
from strtab import S
T = 0x086f82ec
for i in range(200):
    a = T + 0x2c * i
    b = rom()[ra(a):ra(a) + 0x2c]
    nm, ds = struct.unpack_from('<HH', b)
    if S(nm) is None:
        print('end', i); break
    st = struct.unpack_from('<5b', b, 0x16)
    w = struct.unpack_from('<11I', b)
    print(f'{i:3d} {S(nm)[:22]:22} 04={w[1]:x} 08={w[2]:x} 0c={w[3]:08x} 10={w[4]:08x} 14={b[0x14]:02x} 15={b[0x15]:02x} st={st} 1b={b[0x1b]:02x} 1c={w[7]:x} 20={w[8]} 24={w[9]:08x} 28={w[10]:08x}')
