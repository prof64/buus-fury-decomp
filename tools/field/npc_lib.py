"""Shared emulator helpers for the NPC recordings (tools/field/rec_npc.py)."""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import *  # noqa
import emutools  # noqa

PLAYER = 0x03001C34
RNG = 0x03004A5C
FRAME_COUNTER = 0x03000F94
AREA_PTR = 0x03001980
GAMEDATA = 0x03001068
VT_TALK_NPC = 0x0804C92C
VT_NPC = 0x0804CEF4
LOADER = 0x08008B30
TRIG = 0x0203DFA4
EXIT = 0x0203FF40

STEP_VT = {
    0x0804B964: 'walk_to', 0x0804B308: 'walk_to_enemy', 0x0804D4D4: 'wait',
    0x0804B974: 'wander_talk', 0x0804D4E4: 'wander_rect_talk', 0x0804BF80: 'wander_npc',
    0x0804D848: 'wander_rect_npc', 0x0804B318: 'wander_enemy', 0x0804D0C8: 'wander_rect_enemy',
    0x0804B338: 'face_player',
}


def anim_raw(e, p):
    d = e.r32(p + 0x58)
    a = e.r32(p + 0x50)
    n = e.r8(d)
    for r in range(n + 2):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def object_lists(e):
    W = u32(0x080716B4)
    return [u32(0x080716B0), W + 4, W + 0x40C]


def all_objects(e):
    out = []
    for L in object_lists(e):
        n = e.r32(L)
        for k in range(min(n, 256)):
            out.append(e.r32(L + 4 + 4 * k))
    return out


def npc_objects(e):
    """NPC objects in object-list order: (obj, def address unknown -> matched by position later)."""
    seen = []
    for o in all_objects(e):
        if o and e.r32(o) in (VT_TALK_NPC, VT_NPC) and o not in seen:
            seen.append(o)
    return seen


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def step_state(e, o):
    """behaviour runner at obj+0x1B0: [cur step, s8 index, list]; the current step's raw fields"""
    r = o + 0x1B0
    cur = e.r32(r)
    idx = e.r8(r + 4)
    idx = idx - 256 if idx >= 128 else idx
    st = {'index': idx, 'list': hex(e.r32(r + 8))}
    if cur:
        vt = e.r32(cur)
        st['step'] = STEP_VT.get(vt, hex(vt))
        st['words'] = [s32v(e.r32(cur + 4 * i)) for i in range(1, 0x16)]
        st['ptr'] = hex(cur)
    return st


def npc_row(e, o):
    return [hex(o), s32v(e.r32(o + 0x1A0)), s32v(e.r32(o + 0x1A4)), e.r8(o + 0x10), anim_raw(e, o), e.r8(o + 0x11)]


def player_row(e):
    p = e.r32(PLAYER)
    if not p:
        return None
    return [s32v(e.r32(p + 0x1A0)), s32v(e.r32(p + 0x1A4)), e.r8(p + 0x10)]


def rng(e):
    return [e.r32(RNG), e.r32(RNG + 4)]


def arm_warp(e, w, m, x, y, variant=0):
    """warp.py's trick: the snakeway text trigger becomes an exit to (w, m) at (x, y); returns a per-frame fixup"""
    assert e.r32(TRIG) == 0x08049BD4, hex(e.r32(TRIG))
    for i in range(0, 0x20, 4):
        e.w32(EXIT + i, 0)
    e.w8(EXIT + 0x10, 2); e.w8(EXIT + 0x11, w); e.w8(EXIT + 0x12, m); e.w8(EXIT + 0x13, variant)
    e.w16(EXIT + 0x14, x); e.w16(EXIT + 0x16, y)
    e.w32(TRIG, 0x08049BC4)
    e.w8(TRIG + 0x10, 2)
    e.w8(TRIG + 0x11, 0xFF)
    e.w32(TRIG + 0x14, EXIT)
    p = e.r32(PLAYER)
    e.w32(p + 0x1A0, 0x160 << 8)
    e.w32(p + 0x1A4, 0xA0 << 8)

    def fix():
        p = e.r32(PLAYER)
        if p:
            e.w16(EXIT + 8, e.r32(p + 0x1A0) >> 8)
            e.w16(EXIT + 0xA, e.r32(p + 0x1A4) >> 8)
    return fix
