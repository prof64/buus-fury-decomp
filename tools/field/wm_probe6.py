"""wm_probe6: encounter z at creation on the world map: in the 0-50 load frame, step instruction by instruction and
read obj (r4) at 0x0800F72E (enemy ctor 0x0800F68C done, before the enemy-list add)."""
from wm_lib import *  # noqa
import common_field as cf
import rec_worldmap as rw

orig = cf.run_frame
out = []


def rf(e):
    st = e.save()
    n0 = e.r32(ENEMY_LIST)
    info, hs = orig(e)
    if info and info['to'] == [0, 50]:
        e2 = e.save()
        k = 0
        while e.r32(ENEMY_LIST) < 10 and k < 3000000:
            if cf.cur_pc(e) == 0x0800F72E:
                o = e.reg(4)
                out.append([s32v(e.r32(o + 0x1A0)) >> 8, s32v(e.r32(o + 0x1A4)) >> 8, s32v(e.r32(o + 0x1A8)), e.r32(FC)])
            e.step()
            k += 1
        print('stepped', k, 'fc', e.r32(FC), 'n', e.r32(ENEMY_LIST))
        e.hits()
        s4 = e.save()
        ids = [e.breakpoint(a) for a in (0x08003CC8,)]
        e.run(1)
        for h in e.hits():
            if h['kind'] == 'break':
                r = h['regs']
                if r[2] != 0xF0:
                    continue
                print('  bp', hex(h['addr']), 'r0', hex(r[0]), 'r1', hex(r[1]), 'r2', hex(r[2]), 'r3', hex(r[3]), 'stk', [hex(e.r32(0x030079DC + 4 * i)) for i in range(5)], 'lr', hex(r[14]),
                      'r1 words', [hex(e.r32((r[1] & ~3) + 4 * i)) for i in range(4)] if 0x02000000 <= r[1] < 0x09000000 else '')
        for i in ids:
            e.clear(i)
        e.load(s4)
        for idx in (0, 5):
            o = e.r32(ENEMY_LIST + 4 + 4 * idx)
            wid = e.watch(o + 0x1A8, o + 0x1AC, 'w')
            s3 = e.save()
            for _ in range(3):
                e.run(1)
                for h in e.hits():
                    if h['kind'] != 'break':
                        print('  enemy', idx, 'z write pc', hex(h['pc']), 'lr', hex(h['lr']), 'new', h['new'], 'fc', e.r32(FC))
            e.load(s3)
            e.clear(wid)
        e.hits()
        e.load(e2)
    if area(e)[:2] == [0, 50] and e.r32(ENEMY_LIST) > n0 and not out:
        e2 = e.save()
        e.load(st)
        start = e.frame
        while e.frame == start:
            if cf.cur_pc(e) == 0x0800F72E:
                o = e.reg(4)
                out.append([s32v(e.r32(o + 0x1A0)) >> 8, s32v(e.r32(o + 0x1A4)) >> 8, s32v(e.r32(o + 0x1A8))])
            e.step()
        e.hits()
        e.load(e2)
    return info, hs


cf.run_frame = rf
t = cf.record('p6', 'buu_snakeway_start.ss', rw.setup_capsule_8_1, [[0, 40]], '', max_setup=900, frame0_load=2)
for r in out:
    print(r)
print('loads', [(L['f'], L['to']) for L in t['loads']])
