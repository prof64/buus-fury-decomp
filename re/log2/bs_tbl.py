from en_lib import *
import sys
S=lambda v: v-(1<<32) if v>>31 else v
def vel(a,n=4): return [(S(u32(a+8*d)),S(u32(a+8*d+4))) for d in range(n)]
def frame(a):
    b=ROM[(a&0x1ffffff):(a&0x1ffffff)+4]
    return [b[0]-256 if b[0]>127 else b[0], b[1]-256 if b[1]>127 else b[1], b[2], b[3]]
if __name__=='__main__':
    v=int(sys.argv[1],16); f=int(sys.argv[2],16); stride=int(sys.argv[3]) if len(sys.argv)>3 else 12
    print('vel', vel(v))
    for d in range(4): print('frame dir',d, frame(f+stride*d), hex(u32(f+stride*d+8)))
