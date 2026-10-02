"""Vampire / Nosferatu / Vlad grab check: player NOT pinned (the grab moves the player), HP refilled.
Logs AI node entries of the grab node (0x0803399E enter), player controller (obj+0x14), damage events and
EP/HP of the player record per frame around the grab."""
import sys
from common import *
from emutools import cur_pc
from forcespawn import spawn, setpos
from ents import PLAYER, enemies
from damage_model import compute
from rng import Rng

REC = 0x03001560
GRAB_ENTER, ENTRY, RET = 0x0803399E, 0x0800E9D8, 0x0800EAEC
for t in [int(a, 0) for a in sys.argv[1:]] or [0x85]:
    e, o, ok = spawn(t, px=120, py=120)
    e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
    p = e.r32(PLAYER)
    ox, oy = e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256
    setpos(e, p, ox + 40, oy)
    log = []
    pend = {}
    hp_prev = e.r16(REC + 0x16)
    for f in range(1500):
        st = e.save()
        bids = [e.breakpoint(a) for a in (GRAB_ENTER, ENTRY, RET)]
        e.run(1)
        hs = e.hits()
        for b in bids:
            e.clear(b)
        if hs:
            e.load(st)
            start = e.frame
            while e.frame == start:
                pc = cur_pc(e)
                if pc == GRAB_ENTER:
                    log.append((f, 'grab'))
                elif pc == ENTRY:
                    hit = e.reg(1); ad = e.r32(hit + 8); atk = e.r32(hit + 0xc); tg = e.reg(0)
                    pend = dict(rng=(e.r32(0x03004a5c), e.r32(0x03004a60)), kind=e.r8(ad), sub=e.r8(ad + 1),
                                power=e.r32(ad + 4), mult=e.r32(hit + 4), S=e.r16(atk + 0x1b0), P=e.r16(atk + 0x1b2),
                                cb=e.r32(atk + 0x1bc), E=e.r16(tg + 0x1b4), E2=e.r16(tg + 0x1b6),
                                res=e.r8(tg + (0x1b8 if e.r8(ad) == 0 else 0x1b9)), tp=tg == e.r32(PLAYER), ad=hex(ad))
                elif pc == RET and pend:
                    q = pend
                    dmg, c, r = compute(Rng(*q['rng']), q['kind'], q['sub'], q['power'], q['mult'], q['S'], q['P'],
                                        q['cb'], q['E'], q['E2'], q['res'], False, 0)
                    log.append((f, 'dmg', q['tp'], q['ad'], q['mult'], e.r32(e.reg(4) + 0x18), dmg))
                    pend = {}
                e.step()
        hp = e.r16(REC + 0x16)
        if hp != hp_prev:
            log.append((f, 'hp', hp - hp_prev, hex(e.r32(p + 0x14))))
        e.w16(REC + 0x16, 9999)
        hp_prev = 9999
    print(hex(t))
    for l in log[:60]:
        print('  ', l)
