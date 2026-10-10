"""forcespawn.py type [out.ss] -- spawn ANY enemy type (bosses included) on a quiet map.

Method: the Southwest Forest (world 8, map 3) area lists 4 Majin Fighter spawn records (ROM, ctor
0x0800FD55).  Before warping there from buu_snakeway_start.ss, the first record is patched in ROM with
the target's own spawn record (type, gfx id, per-spawn data +0x14..+0x1C), keeping the Majin Fighter's
spawn condition (+4, so story flags don't block it) and position.  The other three records are moved
far away (x,y = 0x7000) and their bodies parked, so the boss is alone with the player.

spawn(t) -> (emu, enemy_obj or None).  Records with no spawn record of their own (e.g. phase-2 forms
created by another enemy) can be forced with gfx=..., or use spawn(t, donor=other_type) to take the
gfx/data of another type."""
import sys
from common import *
from spawns import scan
from warp import warp
from ents import enemies, PLAYER, TYPE_TAB

SW = (8, 3)
_SP = None


def spawn_records():
    global _SP
    if _SP is None:
        _SP = scan()
    return _SP


def etype(e, o):
    return (e.r32(o + 0x224) - TYPE_TAB) // 28


def own_record(t):
    recs = [s for s in spawn_records() if s['type'] == t]
    recs.sort(key=lambda s: s['map'] == 99 and s['world'] == 0)   # prefer real areas
    return int(recs[0]['record'], 16) if recs else None


def setpos(e, o, x, y):
    e.w32(o + 0x1a0, int(x * 256) & 0xffffffff)
    e.w32(o + 0x1a4, int(y * 256) & 0xffffffff)


REC = 0x03001560


def spawn(t, donor=None, px=None, py=None, settle=30, safe=True):
    """safe: give Goku 9999 HP and arrive 150 px from the boss (strong bosses otherwise kill a level-60
    Goku during the warp)."""
    e = emu('buu_snakeway_start.ss')
    if safe:
        e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
    sw = [s for s in spawn_records() if (s['world'], s['map']) == SW and s['type'] == 0x44]
    assert sw, 'no SW forest records'
    tgt = int(sw[0]['record'], 16)
    src = own_record(donor if donor is not None else t)
    if src is not None:
        for off in (0x10, 0x14, 0x18, 0x1c):
            e.w32(tgt + off, u32(src + off))
    e.w32(tgt + 8, t)
    for s in sw[1:]:
        r = int(s['record'], 16)
        e.w16(r + 0xc, 0x7000); e.w16(r + 0xe, 0x7000)
    ex, ey = s16(tgt + 0xc), s16(tgt + 0xe)
    ok = warp(e, SW[0], SW[1], ex - (150 if safe else 60) if px is None else px, ey if py is None else py)
    if safe:
        e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
    e.run(settle)
    obs = [o for o in enemies(e) if etype(e, o) == t]
    for o in enemies(e):
        if o not in obs:
            setpos(e, o, 0x7000, 0x7000)
    return e, (obs[0] if obs else None), ok


if __name__ == '__main__':
    t = int(sys.argv[1], 0)
    e, o, ok = spawn(t)
    p = e.r32(PLAYER)
    print('warp', ok, 'boss obj', hex(o) if o else None, 'record', hex(own_record(t) or 0))
    if o:
        print('boss pos', e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256, 'player', e.r32(p + 0x1a0) / 256,
              e.r32(p + 0x1a4) / 256, 'hp<<10', e.r32(o + 0x28c))
    S = Shots(e, f'spawn_{t:02x}')
    for i in range(6):
        S.snap(); e.run(40)
    S.write()
    if len(sys.argv) > 2:
        e.save_file(WORK / sys.argv[2])
