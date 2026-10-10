from common import *
import sys
tg=set(int(x,16) for x in sys.argv[1:])
for off in range(0,0x30000,2):
    h=ROM[off]|ROM[off+1]<<8
    if h>>11==0b10100:  # add rd, pc, #imm
        t=0x08000000+((off+4)&~3)+(h&0xff)*4
        if t in tg: print(f'{0x08000000+off:08x} adr r{(h>>8)&7}, {t:#x}')
