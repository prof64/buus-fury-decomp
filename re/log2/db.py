from common import *
def cstr(a):
    o=a&0x1ffffff; e=ROM.index(b'\0',o); return ROM[o:e].decode('latin1')
a=0x86f8e40
i=0
while True:
    n,d,x,y,z=struct.unpack_from('<5I',ROM,(a&0x1ffffff)+i*20)
    if n>>24!=8: break
    print(i,hex(a+i*20),cstr(n),x,y,z)
    i+=1
