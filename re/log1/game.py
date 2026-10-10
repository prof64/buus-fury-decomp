"""Game-state helpers for LoG1 (RAM layout discovered in notes.md)."""
from common import *

PLAYER_CTL = 0x030007AC   # player controller struct
CHAR_SIZE = 0x278
ENEMY_TEMPLATES = 0x086B6094


def template_name(t):
    return cstr(u32(ENEMY_TEMPLATES + t * 0x40 + 4))


def ents(e):
    n = e.r32(0x03000610)
    return [e.r32(0x03000614 + 4 * i) for i in range(n)]


def chars(e):
    """(entity, char) for every entity whose data ptr is a char struct."""
    out = []
    for p in ents(e):
        d = e.r32(p + 0x3c)
        if 0x02013224 <= d < 0x02030000 and (d - 0x02013224) % CHAR_SIZE == 0:
            out.append((p, d))
    return out


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def char_info(e, c):
    return dict(type=e.r16(c), team=e.r16(c + 0x9a), alive=e.r16(c + 0x9c),
                hp=s32v(e.r32(c + 0x4c)), maxhp=s32v(e.r32(c + 0x50)), ki=s32v(e.r32(c + 0x54)),
                maxki=s32v(e.r32(c + 0x58)), defense=s32v(e.r32(c + 0x60)), exp=s32v(e.r32(c + 0x84)))


def player_xy(e):
    return s16(e.r16(PLAYER_CTL + 0x14)) >> 4, s16(e.r16(PLAYER_CTL + 0x16)) >> 4


def set_player_xy(e, x, y):
    e.w16(PLAYER_CTL + 0x14, x << 4)
    e.w16(PLAYER_CTL + 0x16, y << 4)


def enemies(e):
    out = []
    for p, c in chars(e):
        ci = char_info(e, c)
        if ci['team'] == 2:
            ci.update(ent=p, char=c, x=s16(e.r16(p + 0x20)), y=s16(e.r16(p + 0x22)), name=template_name(ci['type']))
            out.append(ci)
    return out


def player_char(e):
    return e.r32(e.r32(PLAYER_CTL + 0xC) + 0x3C)
