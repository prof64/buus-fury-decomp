"""script_v2_mkstate.py -- make work/states/buu_v2_yemma.ss (Yemma's Office, via the warp native from Snake Way)."""
from script_v2_lib import *

e, r = run('buu_snakeway_start.ss', [1, 3, 0, 120, 140, ('n', 0x77)], 90)
print('warp', r, 'world/map', e.r8(0x0300197C), e.r8(0x0300197D))
e.save_file(STATES / 'buu_v2_yemma.ss')
shot(e, 'yemma_state')
