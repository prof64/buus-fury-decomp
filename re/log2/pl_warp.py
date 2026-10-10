"""pl_warp.py MAP VAR CHAR [FORM] OUT -- debug Map Test with chosen active character (party byte written before load)."""
from common import *
from warp import find_menu
from pl_common import PARTY
import sys
def warp_char(mi, var, char, form=None, frames=300):
    e=new('log2_debugmenu.ss')
    m=find_menu(e)
    e.w8(PARTY,char)
    if form is not None: e.w8(PARTY+0xc+28*char+8,form)
    e.w32(m+8,2); e.w32(m+0x14,mi); e.w32(m+0x18,var)
    from runto import run_to
    e.set_keys(KEY.A)
    ok=run_to(e,0x08005cc6,400)
    assert ok, 'map test not reached'
    e.w8(PARTY,char)
    if form is not None: e.w8(PARTY+0xc+28*char+8,form)
    e.set_keys(0)
    e.run(frames)
    return e
if __name__=='__main__':
    mi,var,ch=int(sys.argv[1]),int(sys.argv[2]),int(sys.argv[3])
    form=int(sys.argv[4]) if sys.argv[4]!='-' else None
    e=warp_char(mi,var,ch,form)
    # skip dialog
    for i in range(40):
        e.press(KEY.B,2,10)
    e.run(30)
    e.screenshot(OUT+'plw.png')
    P=e.r32(0x3001fa8)
    print('char',e.r8(PARTY),'form',e.r8(PARTY+0xc+28*e.r8(PARTY)+8),'stats',[e.r8(P+0x160+i) for i in range(5)])
    e.save_file(ST+sys.argv[5])
