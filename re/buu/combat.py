"""Combat measurement harness.

combat.py state frames keyspec [dx dy] [--onlyone] [--ep]
  keyspec: e.g. 'A' mash, 'A:hold', 'B:hold:60' (hold 60 then release), 'seq:A,4,0,4;...' custom
Pins enemy #0 at player+(dx,dy) every frame (others parked far away), logs every damage event
(final damage, attack type, multiplier) plus player EP/HP changes.
"""
import sys
from common import *
from emutools import catch
from ents import enemies, PLAYER
from dmglog import DMG_RET, stats

REC = 0x03001560


def setup(e, dx=24, dy=0, park=True):
    p = e.r32(PLAYER)
    en = enemies(e)
    for i, o in enumerate(en):
        if i == 0:
            continue
        if park:
            e.w32(o + 0x1a0, 0x7000 << 8)
            e.w32(o + 0x1a4, 0x7000 << 8)
    return p, en


def pin(e, o, p, dx, dy):
    e.w32(o + 0x1a0, (e.r32(p + 0x1a0) + (dx << 8)) & 0xffffffff)
    e.w32(o + 0x1a4, (e.r32(p + 0x1a4) + (dy << 8)) & 0xffffffff)


def run(e, frames, keyfn, dx=24, dy=0, pinit=True, heal_enemy=True, face=3):
    p, en = setup(e, dx, dy)
    e.w8(p + 0x10, face)          # facing: 0 down, 1 up, 2 left, 3 right
    tgt = en[0] if en else None
    log = []

    def cb(e, pc):
        hit = e.reg(4)
        atk = e.r32(hit + 0xc)
        ad = e.r32(hit + 8)
        log.append(dict(frame=e.frame, who='player' if atk == p else 'enemy', kind=e.r8(ad), sub=e.r8(ad + 1),
                        power=e.r32(ad + 4), mult=e.r32(hit + 4), crit=e.r32(hit + 0x10),
                        dmg=round(e.reg(0) / 1024, 3)))

    hp0 = e.r16(REC + 0x16)
    ep0 = e.r16(REC + 0x1a)
    trace = []
    for f in range(frames):
        if pinit and tgt:
            pin(e, tgt, p, dx, dy)
            if heal_enemy:
                e.w32(tgt + 0x28c, 9999 << 10)
        e.w16(REC + 0x16, e.r16(REC + 0x18))   # keep player alive / measure EP only
        catch(e, [DMG_RET], 1, lambda _f, f=f: keyfn(f), cb)
        trace.append((f, e.r16(REC + 0x1a)))
    return log, trace


def keyfn_from(spec):
    parts = spec.split(':')
    k = 0
    for ch in parts[0].split('+'):
        if ch and ch != '-':
            k |= getattr(KEY, ch)
    if len(parts) > 1 and parts[1] == 'hold':
        n = int(parts[2]) if len(parts) > 2 else 10 ** 9
        return lambda f: k if f < n else 0
    if len(parts) > 1 and parts[1] == 'tap':
        period = int(parts[2]) if len(parts) > 2 else 8
        return lambda f: k if f % period < 2 else 0
    return lambda f: k if f % 8 < 4 else 0


if __name__ == '__main__':
    st = sys.argv[1]
    e = emu(st if (STATES / st).exists() else WORK / st)
    frames = int(sys.argv[2])
    spec = sys.argv[3]
    dx = int(sys.argv[4]) if len(sys.argv) > 4 else 24
    dy = int(sys.argv[5]) if len(sys.argv) > 5 else 0
    if len(sys.argv) > 6:          # skill id [level]: put it in slot 0 and select it
        sk = int(sys.argv[6], 0)
        lv = int(sys.argv[7]) if len(sys.argv) > 7 else 0
        e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0); e.w8(REC + 0x2e, lv); e.w8(REC + 0x2f, lv)
        e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
    log, trace = run(e, frames, keyfn_from(spec), dx, dy)
    for r in log:
        print(r)
    eps = [t[1] for t in trace]
    print('EP trace (every 10f):', eps[::10])
