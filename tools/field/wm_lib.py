"""World-map helpers (fork E): the flying player (class vt 0x0804E1B4, flight state vt 0x0804D038)."""
from common_field import *  # noqa: F401,F403
from common import WORK  # noqa: F401

MAPOBJ = 0x030019A4
SIN = 0x087B6EEC


def raw_anim(e, p):
    """the shown anim struct obj+0x50 -> raw index (def +0x50 + 4 raw)"""
    a = e.r32(p + 0x50)
    d = e.r32(p + 0x58)
    for r in range(0, 40):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def flyer(e):
    p = e.r32(PLAYER)
    if not p:
        return None
    st = e.r32(p + 0x14)
    return {
        'vt': '%08X' % e.r32(p), 'x': s32v(e.r32(p + 0x1A0)), 'y': s32v(e.r32(p + 0x1A4)), 'z': s32v(e.r32(p + 0x1A8)),
        'h': s32v(e.r32(p + 0x1AC)), 'facing': e.r8(p + 0x10), 'raw': raw_anim(e, p), 'aframe': e.r8(p + 0x11),
        'tick': e.r16(p + 0x12), 'st_vt': '%08X' % (e.r32(st) if st else 0),
        'rev': s32v(e.r32(st + 4)) if st else None, 'spd': s32v(e.r32(st + 8)) if st else None,
        'turn': s32v(e.r32(st + 0xC)) if st else None,
    }
