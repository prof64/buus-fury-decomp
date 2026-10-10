"""jspawn(t, bps=()) -- forcespawn.spawn variant for bosses that can kill the player during the warp:
player maxHP/HP set to 9999 and XP to 60,000,000 (level 200: no level-up box) BEFORE the warp, and
breakpoints `bps` installed before the warp so the boss's first AI states are logged too.
Returns (emu, boss_obj, warp_ok)."""
from common import *
from warp import warp
from ents import enemies, PLAYER, TYPE_TAB
import forcespawn as fs

REC = 0x03001560


def jspawn(t, bps=(), dx=-60):
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
    e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
    e.w32(REC + 0x24, 60000000)
    for a in bps:
        e.breakpoint(a)
    ex, ey = s16(tgt + 0xc), s16(tgt + 0xe)
    ok = warp(e, fs.SW[0], fs.SW[1], ex + dx, ey)
    obs = [o for o in enemies(e) if (e.r32(o + 0x224) - TYPE_TAB) // 28 == t]
    for o in enemies(e):
        if o not in obs:
            fs.setpos(e, o, 0x7000, 0x7000)
    return e, (obs[0] if obs else None), ok
