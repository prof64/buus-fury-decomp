from common import *
e = emu('buu_snakeway_start.ss')
print('72720', hex(u32(0x08072720)), hex(u32(0x08072724)))
io = u32(0x08072720)
io2 = e.r32(io)
print('io ptr', hex(io), '->', hex(io2))
for f in range(7):
    e.set_keys(KEY.A if f in (1, 2, 3) else (KEY.A | KEY.B if f == 5 else 0))
    e.run(1)
    print(f, [hex(e.r16(io2 + 2 * i)) for i in range(6)], [hex(e.r16(0x03003300 + 2 * i)) for i in range(6)])
