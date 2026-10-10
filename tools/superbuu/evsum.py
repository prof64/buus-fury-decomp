import re,sys,glob,os
D='work/buu/script/rhai/scripts/events/'
lo,hi=int(sys.argv[1]),int(sys.argv[2])
for i in range(lo,hi):
    p=D+f's{i:03d}.rhai'
    if not os.path.exists(p): continue
    t=open(p).read()
    head=t.split('\n')[0]
    sets=sorted(set(map(int,re.findall(r'set_flag\((\d+)\)',t))))
    clr=sorted(set(map(int,re.findall(r'clear_flag\((\d+)\)',t))))
    tst=sorted(set(map(int,re.findall(r'(?:get_flag|not_flag)\((\d+)\)',t))))
    says=re.findall(r'say: "([^"]{0,60})',t)
    warps=re.findall(r'(warp\w*\([^)]*\))',t)
    party=re.findall(r'((?:join|leave|add_party|remove_party|set_party|party)\w*\([^)]*\))',t)
    print(f's{i:03d}', head.split('Purpose: ')[1] if 'Purpose' in head else head, '| set',sets,'clr',clr,'tst',tst)
    if says: print('    ', says[:2])
    if warps: print('    W', warps[:4])
