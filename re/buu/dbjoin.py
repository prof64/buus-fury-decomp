from common import *
from strtab import S
DB = 0x0874d380
db = []
i = 0
while True:
    a = DB + 16 * i
    nm, ds, x, y, z = struct.unpack_from('<HHIIi', rom(), ra(a))
    if S(nm) is None or nm < 0x300 or x > 0x1000:
        break
    db.append((i, a, S(nm), x, y, z))
    i += 1
print(len(db), 'db entries, end', hex(DB + 16 * len(db)))
byx = {}
for d in db:
    byx.setdefault(d[5], []).append(d)
for d in db:
    print(f'{d[0]:3d} {d[2]:24} X={d[3]:#x} Y={d[4]} Z={d[5]}')
T = 0x08711274
print('--- types')
for t in range(0x140):
    w = struct.unpack_from('<7I', rom(), ra(T + 28 * t))
    names = [(d[2], hex(d[3])) for d in byx.get(t, [])]
    print(f'type {t:#04x} exp?={w[0]} hp={w[1]} def={w[2]:#x} {names}')
