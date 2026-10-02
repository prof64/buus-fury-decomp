"""Find Thumb `ldr rX,[pc,#imm]` instructions that load a given 32-bit value (or from given pool address).
usage: litref.py VALUE_HEX [...]"""
from common import *
import sys
vals=[int(x,16) for x in sys.argv[1:]]
pools={}
for v in vals:
    pat=struct.pack('<I',v); i=ROM.find(pat)
    while i!=-1:
        if i%4==0: pools[i]=v
        i=ROM.find(pat,i+1)
# scan halfwords: thumb ldr literal = 01001 rrr iiiiiiii
code_end=0x200000
for off in range(0,code_end,2):
    h=ROM[off]|ROM[off+1]<<8
    if h>>11==0b01001:
        tgt=((off+4)&~3)+(h&0xff)*4
        if tgt in pools:
            print(f'{0x08000000+off:08x} ldr r{(h>>8)&7}, ={pools[tgt]:#x}')
