"""Run pl_measure.py for every player attack; summaries -> work/log2/pl_measure_summary.txt, raw -> work/log2/pl_meas_*.json"""
import subprocess, sys
PY='/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/.venv/bin/python'
M='/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/log2/pl_measure.py'
O='/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/work/log2/'
G,PI,V,T,K,H=[f'log2_pl_{n}.ss' for n in ('gohan','piccolo','vegeta','trunks','goku','hercule')]
R=[
 # name, state, slot, seq, facing, enemies, flags
 ('punch_r_free',K,3,'W:2 A:1 W:60',3,'14,0','--free --vuln'),
 ('punch_d',K,3,'W:2 A:1 W:30',0,'0,14',''),
 ('punch_u',K,3,'W:2 A:1 W:30',1,'0,-14',''),
 ('punch_l',K,3,'W:2 A:1 W:30',2,'-14,0',''),
 ('blast_r_pierce',K,3,'W:2 B:1 W:60',3,'40,0;70,0',''),
 ('blast_d',K,3,'W:2 B:1 W:60',0,'0,50',''),
 ('blast_u',K,3,'W:2 B:1 W:60',1,'0,-50',''),
 ('blast_l',K,3,'W:2 B:1 W:60',2,'-50,0',''),
 ('blast_free',K,3,'W:2 B:1 W:60',3,'40,0','--free'),
 ('blast_rapid',K,3,'W:2 B:1,W:5*10 W:40',3,'200,0','--vuln'),
 ('masenko_tap',G,0,'W:2 B:1 W:60',3,'34,0;20,0;48,0',''),
 ('masenko_hold30',G,0,'W:2 B:30 W:60',3,'110,0',''),
 ('masenko_d',G,0,'W:2 B:1 W:60',0,'0,34',''),
 ('kame_r_2',K,0,'W:2 B:120 W:60',3,'60,0;90,0','--vuln'),
 ('kame_d',K,0,'W:2 B:60 W:40',0,'0,60',''),
 ('kame_free',K,0,'W:2 B:120 W:60',3,'60,0','--free'),
 ('sbc_r_2',PI,0,'W:2 B:120 W:60',3,'60,0;90,0','--vuln'),
 ('sbc_d',PI,0,'W:2 B:60 W:40',0,'0,60',''),
 ('scatter_r',PI,1,'W:2 B:1 W:70',3,'60,0;60,-30;60,30',''),
 ('scatter_d',PI,1,'W:2 B:1 W:70',0,'0,60;-30,60;30,60',''),
 ('bigbang_tap',V,0,'W:2 B:1 W:80',3,'40,0;40,30;40,-34','--vuln'),
 ('bigbang_hold40',V,0,'W:2 B:40 W:90',3,'70,0;70,31;102,0',''),
 ('bigbang_d',V,0,'W:2 B:1 W:80',0,'0,40',''),
 ('burning_tap',T,0,'W:2 B:1 W:80',3,'40,0;70,0',''),
 ('burning_hold30',T,0,'W:2 B:30 W:80',3,'60,0',''),
 ('burning_free',T,0,'W:2 B:1 W:120',3,'40,0','--free'),
 ('burning_d',T,0,'W:2 B:1 W:80',0,'0,40',''),
 ('swordblast_r',T,1,'W:2 B:1 W:60',3,'16,0;60,0;90,0','--vuln'),
 ('swordblast_d',T,1,'W:2 B:1 W:60',0,'0,60',''),
 ('spirit_hold40',K,1,'W:2 B:40 W:200',3,'100,0;100,30;130,0;60,-20','--vuln'),
 ('spirit_free',K,1,'W:2 B:40 W:200',3,'100,0','--free'),
 ('epunch_r',V,1,'W:2 B:1 W:30',3,'14,0',''),
 ('hercule_sp',H,0,'W:2 B:1 W:60',3,'40,0;-200,0','--free'),
 ('hercule_punch',H,0,'W:2 A:1 W:30',3,'14,0',''),
]
for f,st,nm in ((19,K,'goku'),(2,G,'gohan'),(6,PI,'piccolo'),(8,V,'vegeta'),(12,T,'trunks')):
    R.append((f'spmelee_{nm}',st,3,'W:2 A:110 W:60',3,'26,0','--vuln'))
R.append(('spmelee_goku_short',K,3,'W:2 A:30 W:60',3,'26,0',''))
R.append(('spmelee_piccolo_imm',PI,3,'W:2 A:110 W:60',3,'26,0',''))
R.append(('spmelee_goku_d',K,3,'W:2 A:110 W:60',0,'0,26',''))
R.append(('spirit_hold130',K,1,'W:2 B:130 W:250',3,'100,0;100,40;40,-60',''))
R.append(('spirit_hold10',K,1,'W:2 B:10 W:200',3,'60,0;60,20',''))
only=sys.argv[1:]
out=open(O+'pl_measure_summary.txt','a' if only else 'w')
for name,st,slot,seq,fac,en,fl in R:
    if only and name not in only: continue
    r=subprocess.run([PY,M,name,st,str(slot),seq,str(fac),en]+fl.split(),capture_output=True,text=True)
    out.write(r.stdout+r.stderr+'\n'); out.flush()
    print(name,'ok' if r.returncode==0 else 'FAIL '+r.stderr[-300:])
