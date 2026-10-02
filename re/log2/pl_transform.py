"""pl_transform.py STATE SLOT -- use transform slot, report form/stats/speeds/EP over time; then use it again (revert)."""
from pl_common import *
import sys
e=new(sys.argv[1]); e.run(150); slot=int(sys.argv[2])
R=rec(e); P=player(e)
e.w8(R+9,slot); e.w16(R+4,200); e.w16(R+6,200)
def show(tag):
    P=player(e)
    print(tag,'form',e.r8(R+8),'stats',[e.r8(P+0x160+i) for i in range(5)],'walk',e.r32(P+0x188),'run',e.r32(P+0x18c),'ep',e.r16(R+4),'hp',e.r16(R),'/',e.r16(R+2),'state',pstate(e))
show('before')
e.press(KEY.B,1,0)
for i in range(8):
    e.run(25); show(f't+{(i+1)*25}')
e.w16(R,e.r16(R+2)//2)
for i in range(4):
    e.run(100); show(f'later+{(i+1)*100}')
e.save_file(ST+'log2_pl_transformed.ss')
e.press(KEY.B,1,0)
for i in range(4):
    e.run(40); show(f'revert+{(i+1)*40}')
