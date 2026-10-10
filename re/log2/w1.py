from common import *
import sys
st=sys.argv[1]; a=int(sys.argv[2],16); b=int(sys.argv[3],16); mode=sys.argv[4]; fr=int(sys.argv[5])
keys=int(sys.argv[6]) if len(sys.argv)>6 else 0
e=new(st)
e.watch(a,b,mode)
e.set_keys(keys)
e.run(fr)
seen={}
for h in e.hits():
    k=(h['pc'],h['addr'],h['access'])
    if k not in seen: seen[k]=[0,h]
    seen[k][0]+=1
for k,(n,h) in sorted(seen.items()):
    print(f"pc={h['pc']:08x} lr={h['lr']:08x} addr={h['addr']:08x} {h['access']} w{h['width']} old={h['old']:x} new={h['new']:x} n={n} r0-7={[hex(x) for x in h['regs'][:8]]}")
