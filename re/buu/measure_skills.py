"""Measure every energy skill (table 0x0871DAF8) in the emulator.

For each skill id and level: grant it to Goku (record slot 0), place one enemy 60px below (facing down,
open ground in buu_fight_swforest.ss), then press/hold B with several input patterns. Logs every damage
event the player causes (attack kind/sub, multiplier, damage), the EP spent, the first-hit delay and the
hit cadence. Player HP is refilled each frame so enemy hits do not interrupt. Output:
work/buu/skills_measured.json
"""
import json
import sys
from common import *
from combat import setup, pin, REC
from emutools import catch
from dmglog import DMG_RET
from strtab import S

SK = 0x0871DAF8
PATTERNS = {
    'tap': lambda f: KEY.B if f < 3 else 0,
    'hold60': lambda f: KEY.B if f < 60 else 0,
    'hold150': lambda f: KEY.B if f < 150 else 0,
    'mash': lambda f: KEY.B if (f % 12) < 3 and f < 150 else 0,
}


def measure(skill, level, pattern, frames=260, dist=60, st='buu_fight_swforest.ss'):
    e = emu(st)
    e.w8(REC + 0xc, 1); e.w8(REC + 0xd, skill); e.w8(REC + 0x15, 0)
    e.w8(REC + 0x2e, level); e.w8(REC + 0x2f, level)
    e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
    e.w16(REC + 0x1e, 999); e.w16(REC + 0x20, 999)
    p, en = setup(e)
    e.w8(p + 0x10, 0)
    log = []

    def cb(e, pc):
        hit = e.reg(4)
        if e.r32(hit + 0xc) != p:
            return
        ad = e.r32(hit + 8)
        log.append(dict(f=e.frame - f0, kind=e.r8(ad), sub=e.r8(ad + 1), power=e.r32(ad + 4), mult=e.r32(hit + 4),
                        crit=e.r32(hit + 0x10), dmg=round(e.reg(0) / 1024, 3)))

    f0 = e.frame
    ep = [e.r16(REC + 0x1a)]
    kf = PATTERNS[pattern]
    form0 = e.r8(REC + 0x14)
    for f in range(frames):
        pin(e, en[0], p, 0, dist)
        e.w32(en[0] + 0x28c, 99999 << 10)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        catch(e, [DMG_RET], 1, lambda _f, f=f: kf(f), cb)
        ep.append(e.r16(REC + 0x1a))
    spent = [ep[i - 1] - ep[i] for i in range(1, len(ep))]
    return dict(skill=skill, level=level, pattern=pattern, hits=len(log),
                first_hit=log[0]['f'] if log else None, last_hit=log[-1]['f'] if log else None,
                mults=sorted(set(l['mult'] for l in log)), subs=sorted(set((l['kind'], l['sub']) for l in log)),
                total_dmg=round(sum(l['dmg'] for l in log), 2), ep_total=999 - ep[-1] if ep else 0,
                ep_events=[(i + 1, s) for i, s in enumerate(spent) if s][:40], form_after=e.r8(REC + 0x14), form_before=form0,
                sample=log[:6])


if __name__ == '__main__':
    out = []
    only = [int(x, 0) for x in sys.argv[1:]] if len(sys.argv) > 1 else None
    for sk in range(1, 0x18):
        if only and sk not in only:
            continue
        mx = u32(SK + 24 * sk)
        levels = [0, 1, 2] if mx == 2 else [0]
        for lv in levels:
            for pat in PATTERNS:
                r = measure(sk, lv, pat)
                r['name'] = S(u16(SK + 24 * sk + 12))
                out.append(r)
                print(r['skill'], r['name'], 'lv', lv, pat, 'hits', r['hits'], 'first', r['first_hit'], 'mults', r['mults'],
                      'subs', r['subs'], 'dmg', r['total_dmg'], 'EP', r['ep_total'], 'form', r['form_before'], '->', r['form_after'])
    p = WORK / ('skills_measured.json' if not only else f'skills_measured_{"_".join(map(str, only))}.json')
    p.write_text(json.dumps(out, indent=1))
