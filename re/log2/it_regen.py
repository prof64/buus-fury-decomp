from common import *
import sys
e=new(sys.argv[1] if len(sys.argv)>1 else 'log2_map12b.ss')
P=0x3000e90; idx=e.r8(P); R=P+0xc+28*idx
keys=int(sys.argv[2]) if len(sys.argv)>2 else 0
frames=int(sys.argv[3]) if len(sys.argv)>3 else 600
# move player away from enemies? just test
e.w16(R+4,10); e.w16(R,500)
e.watch(R,R+8,'w')
e.set_keys(keys)
e.run(frames)
hs=e.hits()
last=None
for h in hs:
    k=(h['addr']-R,h['pc'])
    print(f"f{h['frame']} off={h['addr']-R} {h['old']:x}->{h['new']:x} pc={h['pc']:08x} lr={h['lr']:08x}")
print('final hp',e.r16(R),'ep',e.r16(R+4))
