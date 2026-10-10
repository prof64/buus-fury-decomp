from common import *
import time
e = emu('buu_snakeway_start.ss')
a = 0x08046f30
print(hex(e.r16(a)))
e.w16(a, 0x1234)
print('after patch', hex(e.r16(a)))
e.w16(a, u16(a))
# step speed
t = time.time()
for i in range(100000):
    e.step()
print('100k steps', time.time() - t, 'pc', hex(e.reg(15)))
