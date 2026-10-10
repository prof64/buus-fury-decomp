from common import *
STR=0x80e2840
def s(i):
    p=u32(STR+4*i); o=p&0x1ffffff; return ROM[o:ROM.index(b'\0',o)].decode('latin1')
T=0x86ade24
for i in range(50):
    a=T+0x38*i
    b=ROM[a&0x1ffffff:(a&0x1ffffff)+0x38]
    w=struct.unpack_from('<14I',b)
    print(i,b[:4].hex(),s(b[4]|b[5]<<8),'|',s(b[6]|b[7]<<8)[:40],'| fns',hex(w[12]),hex(w[13]))
