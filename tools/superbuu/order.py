import sys
sys.path.insert(0,str(__import__('pathlib').Path(__file__).resolve().parent))
from graph import info, par, go
import glob,os
D=str(__import__('pathlib').Path(__file__).resolve().parents[2]) + '/work/buu/script/rhai/scripts/events/'
evs=sorted(os.path.basename(p)[:-5] for p in glob.glob(D+'*.rhai'))
MAIN=set(range(1,160))
def key(sid, seen=()):
    s,t,o=info(sid)
    m=[f for f in s if f in MAIN]
    if m: return min(m)
    ks=[key(p, seen+(sid,)) for p in par[sid] if p not in seen]
    ks=[k for k in ks if k is not None]
    return min(ks) if ks else None
rows=[]
for sid in evs:
    s,t,o=info(sid)
    if o: rows.append((key(sid), sid, o))
for k,sid,o in sorted(rows, key=lambda r:(r[0] is None, r[0] or 0, r[1])):
    print(k, sid, sorted(par[sid]), o)
