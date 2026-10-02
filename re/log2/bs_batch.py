"""Run bs_watch on a list of (label, class, gfx, dist, frames, axis) and print logs."""
from bs_watch import watch
import sys
CFG={
 'android17_far':(3,9,100,900,'L'),
 'android17_near':(3,9,30,600,'L'),
 'ninja':(75,96,55,900,'L'),
 'bandit':(9,18,70,900,'L'),
 'ladybug':(65,77,50,900,'L'),
 'cellNP_far':(76,29,100,1200,'L'),
 'tao_far':(44,50,100,1200,'L'),
 'destroyer':(31,37,70,900,'L'),
 'cell':(60,28,30,900,'L'),
 'pcell_far':(21,30,100,1200,'L'),
 'hawk':(58,63,40,900,'L'),
 'trex':(99,122,40,900,'L'),
 'wolf':(17,146,20,600,'L'),
 'snake':(53,118,20,600,'L'),
 'trike94':(94,125,10,300,'L'),
}
for k in sys.argv[1:]:
    c=CFG[k]; print('=====',k,c)
    r=watch(*c)
    if r: print('hits',r['hits'])
