"""bv_regular_trace.py type [frames] [dx] [dy] -- force-spawn an enemy, keep the player alive and still at
(dx,dy) px from it, and log for every frame:
  * object spawns 0x0801191A(world, obj): spawn site (LR), object vtable, then the object's raw words
    +0x00..+0x4C each frame while it lives (vtable unchanged)  -> speed / lifetime / bbox
  * hit messages 0x08011C70(world, kind, msg): sender (msg+0x30) vtable, dir, mult, attack type, owner
  * damage to the player: ComputeDamage 0x0800E9D8 entry/exit, predicted with damage_model.compute
Writes work/buu/ptrace/<type>.json and prints a summary."""
import json
import sys
from collections import Counter
from common import *
from emutools import cur_pc
from forcespawn import spawn, setpos
from ents import PLAYER, enemies
from damage_model import compute
from rng import Rng

REC = 0x03001560
SPAWN, SEND, ENTRY, RET = 0x0801191A, 0x08011C70, 0x0800E9D8, 0x0800EAEC
WATCH = [SPAWN, SEND, ENTRY, RET]


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def trace(t, frames=900, dx=60, dy=0, keep_player=True, e=None, o=None):
    if e is None:
        e, o, ok = spawn(t, px=120, py=120)
        e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
    log = dict(type=t, spawns=[], sends=[], damage=[], objs={}, states=[])
    if o is None:
        return log
    p = e.r32(PLAYER)
    ox, oy = s32v(e.r32(o + 0x1a0)) / 256, s32v(e.r32(o + 0x1a4)) / 256
    pend = {}
    live = {}
    pxy = (ox + dx, oy + dy)

    def cb(pc):
        if pc == SPAWN:
            ob = e.reg(1)
            lr = e.reg(14) - 1
            rec = dict(f=f, obj=hex(ob), vt=hex(e.r32(ob)), site=hex(lr), world=hex(e.reg(0)))
            log['spawns'].append(rec)
            live[ob] = dict(vt=e.r32(ob), first=f, site=hex(lr), frames=[])
            log['objs'].setdefault(hex(ob) + '@' + str(f), live[ob])
        elif pc == SEND:
            m = e.reg(2)
            snd = e.r32(m + 0x30)
            ad = e.r32(m + 0x1c)
            rec = dict(f=f, world=hex(e.reg(0)), kind=e.reg(1), lr=hex(e.reg(14) - 1), msgkind=e.r8(m),
                       box=[s32v(e.r32(m + 4 + 4 * k)) for k in range(4)], dir=e.r8(m + 0x14),
                       mult=e.r32(m + 0x18), atk=hex(ad), owner=hex(e.r32(m + 0x20)), sender=hex(snd),
                       sender_vt=hex(e.r32(snd)) if 0x02000000 <= snd < 0x04000000 else None)
            if 0x08000000 <= ad < 0x09000000:
                rec['atk_kind'], rec['atk_sub'], rec['atk_power'] = e.r8(ad), e.r8(ad + 1), e.r32(ad + 4)
            log['sends'].append(rec)
        elif pc == ENTRY:
            tgt, hit = e.reg(0), e.reg(1)
            atk = e.r32(hit + 0xc)
            ad = e.r32(hit + 8)
            pend.clear()
            pend.update(f=f, tgt=tgt, hitp=hit, rng=(e.r32(0x03004a5c), e.r32(0x03004a60)), kind=e.r8(ad),
                        sub=e.r8(ad + 1), power=e.r32(ad + 4), mult=e.r32(hit + 4), S=e.r16(atk + 0x1b0),
                        Pw=e.r16(atk + 0x1b2), cb=e.r32(atk + 0x1bc), E=e.r16(tgt + 0x1b4), E2=e.r16(tgt + 0x1b6),
                        res=e.r8(tgt + (0x1b8 if e.r8(ad) == 0 else 0x1b9)), blk=e.r32(tgt + 0x1c8),
                        bf=e.r16(tgt + 0x1c0), face=e.r8(tgt + 0x10), hdir=e.r8(hit), attacker=hex(atk),
                        atk=hex(ad), lr=hex(e.reg(14) - 1),
                        stack=[hex(w - 1) for w in (e.r32(e.reg(13) + 4 * k) for k in range(40)) if 0x08000000 < w < 0x08100000 and w & 1])
        elif pc == RET and pend:
            q = pend
            blocking = q['blk'] != 0 and (q['hdir'] ^ 1) == q['face']
            dmg, crit, raw = compute(Rng(*q['rng']), q['kind'], q['sub'], q['power'], q['mult'], q['S'], q['Pw'],
                                     q['cb'], q['E'], q['E2'], q['res'], blocking, q['bf'])
            act = e.r32(e.reg(4) + 0x18)
            log['damage'].append(dict(f=q['f'], to_player=q['tgt'] == e.r32(PLAYER), attacker=q['attacker'],
                                      atk=q['atk'], kind=q['kind'], sub=q['sub'], mult=q['mult'],
                                      S=q['S'], POW=q['Pw'], E=q['E'], E2=q['E2'], actual=act, predicted=dmg,
                                      ok=act == dmg, hp_loss=act / 1024, lr=q['lr'], stack=q['stack']))
            pend.clear()

    for f in range(frames):
        if keep_player:
            p = e.r32(PLAYER)
            e.w16(REC + 0x16, e.r16(REC + 0x18))
            setpos(e, p, *pxy)
        st = e.save()
        bids = [e.breakpoint(a) for a in WATCH]
        e.run(1)
        hs = e.hits()
        for b in bids:
            e.clear(b)
        if hs:
            e.load(st)
            start = e.frame
            while e.frame == start:
                pc = cur_pc(e)
                if pc in WATCH:
                    cb(pc)
                e.step()
        for ob, L in list(live.items()):
            if e.r32(ob) != L['vt'] or len(L['frames']) > 400:
                del live[ob]
                continue
            L['frames'].append([s32v(e.r32(ob + k)) for k in range(0, 0x50, 4)])
        if o in enemies(e):
            log['states'].append((f, e.r32(o + 0x14), s32v(e.r32(o + 0x1a0)) / 256, s32v(e.r32(o + 0x1a4)) / 256))
    for L in log['objs'].values():
        L['vt'] = hex(L['vt'])
        L['lived'] = len(L['frames'])
    return log


def summary(log):
    print('type', hex(log['type']), 'spawns', Counter((s['vt'], s['site']) for s in log['spawns']))
    sc = Counter((s['lr'], s['sender_vt'], s['atk'], s['mult'], s['world']) for s in log['sends'])
    for k, v in sc.items():
        print('  send', k, v)
    for d in log['damage']:
        print('  dmg', {k: d[k] for k in ('f', 'to_player', 'atk', 'kind', 'sub', 'mult', 'S', 'POW', 'E2', 'actual', 'predicted', 'ok')})
    for k, L in list(log['objs'].items())[:6]:
        fr = L['frames']
        if len(fr) >= 3:
            # base-class projectile: pos at word 8/9 (+0x20/+0x24), 24.8
            d1 = [((b[8] - a[8]) / 256, (b[9] - a[9]) / 256) for a, b in zip(fr, fr[1:])][:6]
            print('  obj', k, L['vt'], 'site', L['site'], 'lived', L['lived'], 'd(+20,+24)', d1)


if __name__ == '__main__':
    t = int(sys.argv[1], 0)
    frames = int(sys.argv[2]) if len(sys.argv) > 2 else 900
    dx = int(sys.argv[3]) if len(sys.argv) > 3 else 60
    dy = int(sys.argv[4]) if len(sys.argv) > 4 else 0
    log = trace(t, frames, dx, dy)
    (WORK / 'ptrace').mkdir(exist_ok=True)
    (WORK / 'ptrace' / f'{t:02x}_{dx}_{dy}.json').write_text(json.dumps(log))
    summary(log)
