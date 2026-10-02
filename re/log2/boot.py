from common import *
from PIL import Image
e=Emu('log2')
e.run(600)
bp=e.breakpoint(0x08000434)
obj=None
for i in range(40):
    e.press(KEY.START,3,40)
    h=e.hits()
    if h:
        obj=h[-1]['regs'][0]; print('title obj',hex(obj),i); break
e.clear(bp)
e.run(30)
e.screenshot(OUT+'title.png')
e.save_file(ST+'log2_title.ss')
print(e.read(obj,16).hex())
