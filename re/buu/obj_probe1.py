from common import *
import vtab
e = emu('buu_snakeway_start.ss')
e.run(2)
for a in (0x03001960,0x0300195c,0x03001938,0x0300193c,0x03001944,0x0300194c,0x03001954):
    v = e.r32(a)
    print(hex(a), hex(v), [hex(e.r32(a+4*i)) for i in range(1,2)])
