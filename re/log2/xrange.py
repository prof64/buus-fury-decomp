"""Find aligned 32-bit words in ROM (and literal loads in code) pointing into [lo,hi)."""
from common import *
import sys
lo=int(sys.argv[1],16); hi=int(sys.argv[2],16)
for i in range(0,len(ROM)-3,4):
    v=struct.unpack_from('<I',ROM,i)[0]
    if lo<=v<hi: print(hex(0x08000000+i),'->',hex(v))
