"""Dump a cutscene sequence (engine 0x0800b4ae/0x0800b520): array of step ptrs; step = [u16 next, u8 cmd, ...]."""
from en_lib import *
import sys
H=0x083ec848+4
def seq(a, n=40):
    out=[]
    for i in range(n):
        p=u32(a+4*i)
        if p>>24!=8:
            out.append((i,'idx/end',hex(p)))
            if p==0: break
            continue
        nxt=u16(p); cmd=u8(p+2); fn=u32(H+4*cmd)
        raw=ROM[(p&0x1ffffff):(p&0x1ffffff)+16].hex()
        out.append((i,hex(p),f'next={nxt} cmd={cmd} handler={hex(fn)}',raw))
    return out
if __name__=='__main__':
    for a in sys.argv[1:]:
        print('== seq',a)
        for r in seq(int(a,16)): print(*r)
