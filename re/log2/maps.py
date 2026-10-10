from common import *
MAPS=0x8409368; N=0x147
def rec(i):
    o=(MAPS&0x1ffffff)+i*0x38
    return ROM[o:o+8], struct.unpack_from('<12I',ROM,o+8)
if __name__=='__main__':
    for i in range(N):
        ids,f=rec(i)
        print(i, hex(MAPS+i*0x38), ids.hex(' '), ' '.join(hex(x) for x in f))
