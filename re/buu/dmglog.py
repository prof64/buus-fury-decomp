"""dmglog.py state frames keys_pattern -- log every damage computation (Character::ComputeDamage
0x0800e9d8): attacker/target stats, attack data, crit, raw and final damage.
keys_pattern e.g. 'A' (mashed 4 on/4 off), 'B:hold', '-' none."""
import sys
from common import *
from emutools import catch
from ents import TYPE_TAB, PLAYER

DMG_FN = 0x0800e9d8
DMG_RET = 0x0800eabe     # str r0,[r4,#0x18] (final damage written)


def stats(e, o):
    b = e.read(o + 0x1b0, 0x14)
    return dict(STR=b[0] | b[1] << 8, POW=b[2] | b[3] << 8, END=b[4] | b[5] << 8, END2=b[6] | b[7] << 8,
                resM=b[8], resE=b[9], critb=int.from_bytes(b[0xc:0x10], 'little'))


def logger(out):
    def cb(e, pc):
        if pc == DMG_RET:
            hit = e.reg(4)
            atk = e.r32(hit + 0xc)
            tgt = e.reg(6)
            ad = e.r32(hit + 8)
            out.append(dict(frame=e.frame, attacker=hex(atk), target=hex(tgt), attack_data=hex(ad),
                            ad_bytes=e.read(ad, 8).hex(), mult=e.r32(hit + 4), crit=e.r32(hit + 0x10),
                            raw=e.r32(hit + 0x14), dmg=e.reg(0), dmg_hp=round(e.reg(0) / 1024, 3),
                            atk_stats=stats(e, atk), tgt_stats=stats(e, tgt)))
    return cb


def keyfn_from(pat):
    if pat == '-':
        return lambda f: 0
    k = 0
    for ch in pat.split(':')[0].split('+'):
        k |= getattr(KEY, ch)
    if pat.endswith(':hold'):
        return lambda f: k
    return lambda f: k if f % 8 < 4 else 0


if __name__ == '__main__':
    st = sys.argv[1]
    e = emu(st if (STATES / st).exists() else WORK / st)
    frames = int(sys.argv[2])
    pat = sys.argv[3] if len(sys.argv) > 3 else 'A'
    out = []
    catch(e, [DMG_RET], frames, keyfn_from(pat), logger(out))
    print('player', hex(e.r32(PLAYER)))
    for rec in out:
        print(rec)
