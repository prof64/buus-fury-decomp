"""bv_buu_show.py fn [fn..] [-sec create|vt0|vt1|vt5|ctrl] -- print sections of work/buu/bv_buu_nodes_<fn>.txt"""
import sys
from common import *

args = [a for a in sys.argv[1:] if not a.startswith('-')]
secs = [a[1:] for a in sys.argv[1:] if a.startswith('-')]
for a in args:
    fn = int(a, 16)
    txt = (WORK / f'bv_buu_nodes_{fn:08x}.txt').read_text().split('\n')
    print('########', hex(fn))
    cur = 'auto'
    for ln in txt:
        if ln.startswith('--- create'):
            cur = 'create'
        elif ln.startswith('vtable'):
            cur = 'vtline'
        elif ln.startswith('--- vt['):
            cur = 'vt' + ln[7]
        elif ln.startswith('controller'):
            cur = 'ctrl'
        if not secs or cur in secs or cur == 'vtline':
            print(ln)
