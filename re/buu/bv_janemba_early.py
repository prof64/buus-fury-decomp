"""bv_janemba_early.py type frames [kill_minis_at] -- like forcespawn.spawn but breakpoints are installed
before the warp, so the boss's AI state changes are timed from its creation.  Logs (frame, obj, state)
for goto/push, hand/mini spawns, damage calls on the player; keeps the player alive.  If kill_minis_at is
given, every Mini-Janemba (0x38) is given 0 HP... (hp<<10 = 1) at that frame so the 'wait for minis'
node can be observed finishing.
Also prints the boss part chain: boss+0x1F0 -> +0x1F8 -> +0x1F8 with types."""
import sys
from common import *
from spawns import scan
from warp import warp
from ents import enemies, PLAYER, TYPE_TAB
import forcespawn as fs

REC = 0x03001560
t = int(sys.argv[1], 0)
frames = int(sys.argv[2])
kill_at = int(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3] != '-' else None
hpfrac = float(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] != '-' else None
e = emu('buu_snakeway_start.ss')
sw = [s for s in fs.spawn_records() if (s['world'], s['map']) == fs.SW and s['type'] == 0x44]
tgt = int(sw[0]['record'], 16)
src = fs.own_record(t)
if src is not None:
    for off in (0x10, 0x14, 0x18, 0x1c):
        e.w32(tgt + off, u32(src + off))
e.w32(tgt + 8, t)
for s in sw[1:]:
    r = int(s['record'], 16)
    e.w16(r + 0xc, 0x7000); e.w16(r + 0xe, 0x7000)
ex, ey = s16(tgt + 0xc), s16(tgt + 0xe)
BP = {0x0800AA4A: 'goto', 0x0800AA9C: 'push', 0x0800E9D8: 'dmg', 0x0800F68C: 'create'}
for a in BP:
    e.breakpoint(a)
ok = warp(e, fs.SW[0], fs.SW[1], ex - 60, ey)
f0 = None


def ty(o):
    try:
        return (e.r32(o + 0x224) - TYPE_TAB) // 28
    except Exception:
        return None


def dump(hs):
    global f0
    for h in hs:
        a, r, fr = h['addr'], h['regs'], h['frame']
        if a == 0x0800F68C:
            if f0 is not None:
                print(f'{fr - f0:5d} create type {(r[2] - TYPE_TAB) // 28:#x} lr {h["lr"]:#x}')
            continue
        if a in (0x0800AA4A, 0x0800AA9C):
            o = r[0] - 0x238
            if f0 is None and ty(o) == t:
                f0 = fr
            print(f'{fr - (f0 or fr):5d} {BP[a]} obj {o:#x} type {ty(o)} state {r[1]:#x}')
        elif a == 0x0800E9D8:
            if r[0] == e.r32(PLAYER):
                hit = r[1]
                print(f'{fr - (f0 or fr):5d} dmg->player hitptr {hit:#x}')


HIT = 0x0203FE00


def kill_all(typ):
    """Apply a lethal hit to every enemy of type typ through the normal enemy damage-apply path
    0x0800FA2A(obj, hit) (hit+0x18 = damage in 1/1024 HP), injected with emutools.call."""
    from emutools import call
    for o in [o for o in enemies(e) if ty(o) == typ]:
        for i in range(0, 0x20, 4):
            e.w32(HIT + i, 0)
        e.w32(HIT + 4, 0x400); e.w32(HIT + 8, 0x086B8098); e.w32(HIT + 0xC, e.r32(PLAYER))
        e.w32(HIT + 0x18, 0x7FFFFFF)
        call(e, 0x0800FA2A, (o, HIT, 0, 0))
        dump(e.hits())


dump(e.hits())
boss =[o for o in enemies(e) if ty(o) == t]
if boss:
    b = boss[0]
    c1 = e.r32(b + 0x1f0); c2 = e.r32(c1 + 0x1f8) if c1 else 0; c3 = e.r32(c2 + 0x1f8) if c2 else 0
    print('chain', hex(b), '->', [(hex(c), ty(c)) for c in (c1, c2, c3) if c])
    if hpfrac is not None:
        e.w32(b + 0x28c, int(u32(TYPE_TAB + 28 * t + 4) * hpfrac) << 10)
SN = set(range(380, 760, 24)) if len(sys.argv) > 5 else None
SH = Shots(e, f'jearly_{t:02x}')
e.w32(REC + 0x24, 60000000)   # max level: no level-up box pausing the game
for f in range(frames):
    e.run(1)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    if kill_at is not None and f == kill_at:
        kill_all(0x38); kill_all(0x44)
    if kill_at is not None and f in (kill_at + 5, kill_at + 60, kill_at + 300, kill_at + 900):
        print('roster', f, 'rel', e.frame - f0, [hex(ty(o)) for o in enemies(e)])
        W = 0x03001c4c
        n = e.r32(W + 4)
        objs = [e.r32(W + 8 + 4 * i) for i in range(min(n, 200))]
        print('  world objs with mini def:', [(hex(o), hex(e.r32(o))) for o in objs if e.r32(o + 0x58) == 0x086af018],
              'n', n)
    dump(e.hits())
    if SN is not None and f in SN:
        SH.snap()
e.save_file(WORK / f'jan_end_{t:02x}.ss')
if SN is not None:
    SH.write()
