"""warp.py MAPIDX [VARIATION] [OUTSTATE] -- use debug Map Test to load a map (all chars set to Lv40, all moves)."""
from common import *
import sys
def find_menu(e):
    d=e.ewram(); i=d.find(struct.pack('<I',0x8023ee8))
    while i!=-1 and i%4: i=d.find(struct.pack('<I',0x8023ee8),i+1)
    return 0x02000000+i
def warp(mi, var=0, frames=240):
    e=new('log2_debugmenu.ss')
    m=find_menu(e)
    e.w32(m+8,2); e.w32(m+0x14,mi); e.w32(m+0x18,var)
    e.press(KEY.A,3,3)
    e.run(frames)
    return e
if __name__=='__main__':
    mi=int(sys.argv[1]); var=int(sys.argv[2]) if len(sys.argv)>2 else 0
    e=warp(mi,var)
    e.screenshot(OUT+'warp.png')
    if len(sys.argv)>3: e.save_file(ST+sys.argv[3])
