"""fxB_spawnlog.py mode frames [ehp] -- log every generic effect spawn (0x0802A81C) and ctor-style spawns with caller,
record table, frame count, rate (stack arg 0), stack arg 2 (prio), position. Modes as fxB_melee.py
(punch/kill/dash/block/hurt) or 'skill:<id>:<hold>' (B held), state file optional arg 4."""
import sys
from common import *
from combat import setup, pin, REC
from emutools import catch

mode = sys.argv[1]
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 120
ehp = float(sys.argv[3]) if len(sys.argv) > 3 else 9999
st = sys.argv[4] if len(sys.argv) > 4 else 'buu_fight_swforest.ss'
e = emu(st)
sk = None
if mode.startswith('skill'):
    _, sk, hold = mode.split(':')
    sk, hold = int(sk), int(hold)
    e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
    e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, 3)
SPAWN = 0x0802A81C
EXPL = 0x0801952C
RAND = 0x0803FACC


def keys(f):
    if sk is not None:
        return KEY.B if f < hold else 0
    if mode in ('punch', 'kill'):
        return KEY.A if f % 8 < 3 else 0
    if mode == 'dash':
        return KEY.RIGHT if (f < 3 or 6 <= f < 60) else 0
    if mode == 'block':
        return KEY.R
    return 0


def cb(e, pc):
    sp = e.reg(13)
    if pc == SPAWN:
        xy = e.reg(1)
        print(f'  f{e.frame} gt{e.r32(0x03000f94) % 1000} SPAWN lr={e.reg(14):08x} tbl={e.reg(2):08x} n={e.reg(3)} '
              f'rate={e.r32(sp):#x} a1={e.r32(sp + 4):#x} prio={e.r32(sp + 8):#x} xy=({e.r32(xy)},{e.r32(xy + 4)}) '
              f'player=({e.r32(p + 0x1a0) >> 8},{e.r32(p + 0x1a4) >> 8})')
    elif pc == EXPL:
        print(f'  EXPL(0x0801952c) lr={e.reg(14):08x} prio={e.reg(2):#x}')
    else:
        print(f'  f{e.frame} gt{e.r32(0x03000f94) % 1000} rand({e.reg(0)}) lr={e.reg(14):08x}')


for f in range(frames):
    if en and mode in ('punch', 'block', 'hurt', 'kill'):
        pin(e, en[0], p, 20, 0)
        if mode != 'kill':
            e.w32(en[0] + 0x28c, int(ehp * 1024))
        elif f == 0:
            e.w32(en[0] + 0x28c, int(ehp * 1024))
    elif en and sk is not None:
        pin(e, en[0], p, 60, 0)
        e.w32(en[0] + 0x28c, int(ehp * 1024))
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    catch(e, [SPAWN, RAND, EXPL], 1, lambda _f, f=f: keys(f), cb)
