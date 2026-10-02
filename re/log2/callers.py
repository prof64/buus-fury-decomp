"""Find Thumb BL instructions targeting given addresses."""
from common import *
import sys
tg=set(int(x,16)&~1 for x in sys.argv[1:])
end=0x30000
for off in range(0,end,2):
    h1=ROM[off]|ROM[off+1]<<8
    if h1>>11==0b11110:
        h2=ROM[off+2]|ROM[off+3]<<8
        if h2>>11==0b11111:
            hi=h1&0x7ff
            if hi&0x400: hi-=0x800
            t=0x08000000+off+4+(hi<<12)+((h2&0x7ff)<<1)
            if t in tg: print(f'{0x08000000+off:08x} bl {t:#x}')
