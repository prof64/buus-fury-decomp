"""combat_trace.py STATE frames cmds... : run inputs while logging calls to hit handler (0x08007416)
and damage function (0x08007352) with their args, plus HP writes."""
import sys
from game import *

HIT = 0x08007416
DMG = 0x08007352


def trace(e, frames_cmds):
    e.breakpoint(HIT)
    e.breakpoint(DMG)
    log = []
    for keys, n in frames_cmds:
        for _ in range(n):
            e.set_keys(keys)
            e.run(1)
            for h in e.hits():
                if h['kind'] != 'break':
                    continue
                r = h['regs']
                sp = r[13]
                st = [e.r32(sp + 4 * i) for i in range(4)]
                log.append((h['frame'], h['pc'] - 4, r[:4], st, r[14]))
    return log


if __name__ == '__main__':
    e = emu(sys.argv[1])
    cmds = []
    for c in sys.argv[2:]:
        k, n = c.split(':')
        keys = 0
        if k != '0':
            for nm in k.split('+'):
                keys |= getattr(KEY, nm)
        cmds.append((keys, int(n)))
    pc0 = player_char(e)
    before = {en['char']: en['hp'] for en in enemies(e)}
    for f, pc, r, st, lr in trace(e, cmds):
        print(f, hex(pc), 'r0-3', [hex(x) for x in r], 'stack', st, 'lr', hex(lr))
    for en in enemies(e):
        if before.get(en['char']) != en['hp']:
            print('HP change', en['name'], hex(en['char']), before.get(en['char']), '->', en['hp'])
    print('player', char_info(e, pc0))
    e.screenshot(SHOTS / 'ct.png', scale=1)
