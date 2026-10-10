"""pl_switch.py IN OUT CHAR -- try switching active character via the in-game Switch menu path.
Approach: call the same code the menu uses? First test: write party byte then force player refresh by state."""
from pl_common import *
import sys
e=new(sys.argv[1]); c=int(sys.argv[3])
e.w8(PARTY,c)
e.run(30)
e.screenshot(OUT+'sw.png')
P=player(e)
print('stats',[e.r8(P+0x160+i) for i in range(5)],'form',e.r8(rec(e)+8),'anim',hex(e.r32(P+0x48)))
e.save_file(ST+sys.argv[2])
