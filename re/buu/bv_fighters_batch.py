"""Batch of forced-state measurements for the fighters group -> work/buu/fighters_runs/<name>.txt
usage: bv_fighters_batch.py [name ...]"""
import sys
from bv_fighters_lib import *

RUNS = {
    # name: (type, state, dist, frames, extra)
    'mvegeta_161': (0x46, 0x161, 80, 400, {}),
    'mvegeta_160': (0x46, 0x160, 80, 300, {}),
    'mvegeta_free': (0x46, None, 60, 1500, {}),
    'vegeta88_311': (0x88, 0x311, 40, 400, {}),
    'vegeta89': (0x89, None, 60, 400, {}),
    'broly_free': (0x0a, None, 60, 1500, {}),
    'broly_hp10': (0x0a, None, 60, 600, dict(hp_frac=0.09)),
    'broly2_free': (0x0b, None, 90, 2500, dict(donor=0x0a)),
    'broly2_4c': (0x0b, 0x4c, 90, 500, dict(donor=0x0a)),
    'broly2_49': (0x0b, 0x49, 90, 300, dict(donor=0x0a)),
    'broly_3a': (0x0a, 0x3a, 90, 300, {}),
    'dabura_free': (0x12, None, 60, 1500, {}),
    'yakon_free': (0x91, None, 60, 1500, {}),
    'yakon_321': (0x91, 0x321, 90, 600, {}),
    'yakon_326': (0x91, 0x326, 60, 600, {}),
    'yakon_324': (0x91, 0x324, 60, 600, {}),
    'miniyakon': (0x92, None, 60, 600, {}),
    'spopo6f_22e': (0x6f, 0x22e, 10, 400, {}),
    'spopo6f_free': (0x6f, None, 40, 1200, {}),
    'yamu93_free': (0x93, None, 40, 1200, {}),
    'yamu93_334': (0x93, 0x334, 40, 400, {}),
    'puipui_free': (0x60, None, 60, 900, {}),
    't0d_free': (0x0d, None, 60, 900, {}),
    't0e_free': (0x0e, None, 60, 600, {}),
}

D = WORK / 'fighters_runs'
D.mkdir(exist_ok=True)
names = sys.argv[1:] or list(RUNS)
for n in names:
    t, st, dist, frames, extra = RUNS[n]
    try:
        e, o, L = fight(t, frames, dist=dist, state=st, **extra)
        hdr = f'{n}: type {t:#x} state {st} dist {dist} frames {frames} -> boss hp {hp(e, o)} pos {pos(e, o)} ' \
              f'player {pos(e, e.r32(PLAYER))}'
        txt = hdr + '\n' + summarize(L, o)
    except Exception as ex:  # noqa
        txt = f'{n}: FAILED {ex!r}'
    (D / f'{n}.txt').write_text(txt)
    print(txt.split('\n')[0])
