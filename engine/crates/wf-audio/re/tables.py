import struct
from pathlib import Path
ROM = (Path(__file__).resolve().parents[4] / 'roms' / 'buu.gba').read_bytes()
def u32(a): return struct.unpack_from('<I', ROM, a - 0x08000000)[0]
def u16(a): return struct.unpack_from('<H', ROM, a - 0x08000000)[0]
def s8(a): return struct.unpack_from('<b', ROM, a - 0x08000000)[0]
print('effect table 0x087b6904:')
for i in range(64):
    v = u32(0x087b6904 + 4 * i)
    print(i, hex(v))
    if not (0x08000000 <= v < 0x08800000):
        break
print('porta table 0x087b6ec4:', [hex(u32(0x087b6ec4 + 4 * i)) for i in range(6)])
print('pitch 0x087b6724:', [hex(u32(0x087b6724 + 4 * i)) for i in range(120)])
print('up 0x087b6a04-0x80:', [u16(0x087b6a04 - 0x80 + 2 * i) for i in range(64)])
print('up 0x087b6a04:', [u16(0x087b6a04 + 2 * i) for i in range(256)])
print('dn 0x087b6c84-0x80:', [u16(0x087b6c84 - 0x80 + 2 * i) for i in range(64)])
print('dn 0x087b6c84:', [u16(0x087b6c84 + 2 * i) for i in range(256)])
print('sine 0x087b6e84:', [s8(0x087b6e84 + i) for i in range(64)])
print('0x087b6720 =', hex(u32(0x087b6720)))
for i in range(53):
    a = 0x083BB78C + 20 * i
    p, o, ins, fl = (u32(a + 4 * k) for k in range(4))
    tempo, speed, b2, b3 = ROM[a + 16 - 0x08000000:a + 20 - 0x08000000]
    print(i, hex(p), hex(o), hex(ins), hex(fl), tempo, speed, b2, b3)
