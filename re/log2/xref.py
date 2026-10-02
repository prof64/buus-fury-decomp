from common import *
import sys
# find 32-bit pointers to given addresses
for t in sys.argv[1:]:
    v=int(t,16)
    pat=struct.pack('<I',v)
    i=ROM.find(pat); res=[]
    while i!=-1:
        res.append(hex(0x08000000+i)); i=ROM.find(pat,i+1)
    print(t, res[:30])
