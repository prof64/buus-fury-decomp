"""Run the move battery for every character/slot; writes work/log2/pl_bat_<name>.txt"""
import subprocess, sys
PY='/Users/prof/source/buus-fury-decomp/.venv/bin/python'
B='/Users/prof/source/buus-fury-decomp/re/log2/pl_battery.py'
O='/Users/prof/source/buus-fury-decomp/work/log2/'
runs=[
 ('goku_s1_spirit_hold200','log2_pl_goku.ss',1,'W:2 B:200 W:200',60),
 ('goku_s1_spirit_hold40','log2_pl_goku.ss',1,'W:2 B:40 W:200',60),
 ('goku_s2_transform','log2_pl_goku.ss',2,'W:2 B:1 W:120',60),
 ('goku_meleeA','log2_pl_goku.ss',3,'W:2 A:1 W:30 A:1 W:30',20),
 ('goku_specmelee','log2_pl_goku.ss',3,'W:2 A:120 W:80',24),
 ('gohan_s0_masenko_tap','log2_pl_gohan.ss',0,'W:2 B:1 W:120',60),
 ('gohan_s0_masenko_hold60','log2_pl_gohan.ss',0,'W:2 B:60 W:120',60),
 ('gohan_s1_kame','log2_pl_gohan.ss',1,'W:2 B:90 W:60',60),
 ('gohan_specmelee','log2_pl_gohan.ss',3,'W:2 A:120 W:80',24),
 ('piccolo_s0_sbc','log2_pl_piccolo.ss',0,'W:2 B:90 W:60',60),
 ('piccolo_s1_scatter','log2_pl_piccolo.ss',1,'W:2 B:1 W:90',50),
 ('piccolo_s2','log2_pl_piccolo.ss',2,'W:2 B:1 W:120',60),
 ('piccolo_specmelee','log2_pl_piccolo.ss',3,'W:2 A:120 W:80',24),
 ('vegeta_s0_bigbang_hold90','log2_pl_vegeta.ss',0,'W:2 B:90 W:120',60),
 ('vegeta_s0_bigbang_tap','log2_pl_vegeta.ss',0,'W:2 B:1 W:120',60),
 ('vegeta_s1_epunch','log2_pl_vegeta.ss',1,'W:2 B:1 W:40 B:1 W:40',20),
 ('vegeta_specmelee','log2_pl_vegeta.ss',3,'W:2 A:120 W:80',24),
 ('trunks_s0_burning','log2_pl_trunks.ss',0,'W:2 B:1 W:120',60),
 ('trunks_s1_swordblast','log2_pl_trunks.ss',1,'W:2 B:1 W:90',50),
 ('trunks_specmelee','log2_pl_trunks.ss',3,'W:2 A:120 W:80',24),
 ('hercule_s0','log2_pl_hercule.ss',0,'W:2 B:1 W:90',30),
 ('hercule_meleeA','log2_pl_hercule.ss',0,'W:2 A:1 W:30',20),
 ('basic_rapid','log2_pl_goku.ss',3,'W:2 B:1,W:6*8 W:40',80),
]
runs+=[
 ('trunks_s1_swordblast_far','log2_pl_trunks.ss',1,'W:2 B:1 W:90',150),
 ('trunks_s0_burning_far','log2_pl_trunks.ss',0,'W:2 B:1 W:120',150),
 ('trunks_s0_burning_hold30','log2_pl_trunks.ss',0,'W:2 B:30 W:120',150),
 ('piccolo_s1_scatter_far','log2_pl_piccolo.ss',1,'W:2 B:1 W:90',150),
 ('piccolo_s0_sbc_far','log2_pl_piccolo.ss',0,'W:2 B:150 W:60',150),
 ('goku_s0_kame_far','log2_pl_goku.ss',0,'W:2 B:150 W:60',150),
 ('vegeta_s0_bigbang_far','log2_pl_vegeta.ss',0,'W:2 B:30 W:120',150),
 ('goku_s1_spirit_far','log2_pl_goku.ss',1,'W:2 B:40 W:250',150),
 ('vegeta_s2_transform','log2_pl_vegeta.ss',2,'W:2 B:1 W:200',150),
 ('gohan_s2_transform','log2_pl_gohan.ss',2,'W:2 B:1 W:200',150),
 ('trunks_s2_transform','log2_pl_trunks.ss',2,'W:2 B:1 W:200',150),
]
only=sys.argv[1:]
for name,st,slot,seq,dist in runs:
    if only and name not in only: continue
    out=subprocess.run([PY,B,st,str(slot),seq,str(dist)],capture_output=True,text=True)
    open(O+f'pl_bat_{name}.txt','w').write(out.stdout+out.stderr)
    print(name,'done',len(out.stdout))
