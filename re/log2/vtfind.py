"""Find relative vtables containing method target T: word at base+4k == T-base."""
from common import *
import sys
T=[int(x,16)|1 for x in sys.argv[1:]]
for A in range(0x08020000,0x08030000,4):
    for k in range(0,64):
        w=u32(A+4*k)
        for t in T:
            if (A+w)&0xffffffff==t: print(f'base {A:#x} slot {k} -> {t:#x}')
