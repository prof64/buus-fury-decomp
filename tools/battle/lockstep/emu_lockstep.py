"""Buu's Fury lockstep recorder: runs one scenario in the emulator and records, for every frame, what the player
actually does and shows, plus every sound call. The simulator replays the same scenario frame by frame
(site/tests/battle/buu_lockstep.test.ts) and reports the first divergence. Format and usage: tools/battle/LOCKSTEP.md.

Per frame (after the frame's input was applied and the game logic ran):
  x, y          player position, 24.8 (obj+0x1A0 / +0x1A4)
  dir           obj+0x10 (0 down, 1 up, 2 left, 3 right)
  raw           animation shown: index r with def[+0x50 + 4r] == obj+0x50 (def = obj+0x58), = sprites.json raw_index
  frame, tick   obj+0x11 frame index, obj+0x12 tick (u16)
  state         the player state object at obj+0x14, named by its vtable (STATE_NAMES; unknown -> 'vt_XXXXXXXX')
  form          record +0x14;  hp / ep: record +0x16 / +0x1A
  sfx           PlaySfx 0x0803E378 (id = entry index of the SFX table 0x083192A8, volume), StopSfx 0x0803E302,
                StopAll 0x0803E2EC, PlaySong 0x0803F110 (song table 0x083BB78C)
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import emu, KEY, u8, u16, u32, s32  # noqa: E402
from emutools import call  # noqa: E402
from warp import warp  # noqa: E402

PLAYER = 0x03001C34
GD = 0x03001068
REC0 = 0x03001560
RNG = 0x03004A5C
FORMS = 0x086B82EC
SFX_TABLE = 0x083192A8
SONG_TABLE = 0x083BB78C
PLAY_SFX = 0x0803E378
STOP_SFX = 0x0803E302
STOP_ALL = 0x0803E2EC
STOP_MATCH = 0x0803E32C
PLAY_SONG = 0x0803F110
PLAYER_MSG = 0x08010DDE  # player vfunc +0x24 (obj, kind, msg)
ATK_TABLE = 0x086B8088
# scratch EWRAM for injected hits (checked unused in the HTC map)
MSG = 0x0203FE00
FAKE_VT = 0x0203FE80
ATTACKER = 0x0203FC00
BX_LR = 0x0800DE6B  # a Thumb 'bx lr'

KEYS = {'A': KEY.A, 'B': KEY.B, 'SELECT': KEY.SELECT, 'START': KEY.START, 'RIGHT': KEY.RIGHT, 'LEFT': KEY.LEFT,
        'UP': KEY.UP, 'DOWN': KEY.DOWN, 'R': KEY.R, 'L': KEY.L}

# player state objects by vtable (filled in while decoding; see LOCKSTEP.md)
STATE_NAMES = {
    # singletons (IWRAM): the generic anim states of SetAnim(id) (table 0x0807273C) and the player's own
    0x0804C790: 'idle',       # 0x03002B44 (form controller's idle; random start tick, blink)
    0x0804A77C: 'idle',       # 0x03001B0C (idle of the fusion-variant controller 0x03001B64)
    0x0804C3A8: 'walk',       # 0x03002B4C
    0x0804BEF0: 'dash',       # 0x03002B3C
    0x0804E00C: 'autowalk',   # 0x03002B54 (area transitions)
    0x0804C384: 'bored',      # 0x03002B38 (anim id 8 after 1024 idle frames)
    0x0804C35C: 'punch',      # 0x03001B50 (anim ids 4..6)
    0x0804A188: 'hold0',      # 0x03001B04 (frame 0, no tick)
    0x0804985C: 'hold',       # 0x03001B00 (frame kept, no tick)
    0x0804A1AC: 'loop5from2', # 0x03001B08
    0x0804D784: 'once',       # 0x03001B10 / 0x03001B18 (shift 3 / 4, then idle)
    0x0804C720: 'loop',       # 0x03001B20 / 0x03001B28
    0x0804E0AC: 'once_free',  # 0x03001B30 / 0x03001B38
    0x0804A730: 'once_hold',  # 0x03001B40 (knockdown: shift 5, then hold)
    0x08049E40: 'loop_b',     # 0x03001B48
    0x0804A1D0: 'loop_c',     # 0x03001B58
    0x0804A214: 'none',       # 0x03001BA4
    0x0804CC58: 'ss_revert',  # 0x03002BB4 (anim id 24)
    0x0804DCA4: 'ss_up',      # 0x03002BBC (anim id 23)
    # heap states
    0x0804C744: 'block',
    0x0804D75C: 'hurt',       # also the dash wall bump
    0x0804C768: 'ko',
    0x0804E2B0: 'skill20',    # Super Saiyan transformation
    0x0804DE7C: 'skill1', 0x0804D05C: 'skill2', 0x0804E244: 'skill3', 0x0804DF18: 'skill4', 0x0804D9B0: 'skill5',
    0x0804E2D8: 'skill6', 0x0804DC14: 'skill7', 0x0804DC38: 'skill8', 0x0804DC5C: 'skill9', 0x0804D9D4: 'skill10',
    0x0804E088: 'skill11', 0x0804DC80: 'skill13', 0x0804D9F8: 'skill15', 0x0804DA1C: 'skill16', 0x0804D080: 'skill17',
    0x0804DFE8: 'skill23',
    0x0804E334: 'skill14',    # Instant Transmission teleport sequence (after the target pick)
    0x0804D410: 'skill18',    # Saiyaman: change into form 14
    0x0804DFC0: 'skill18r',   # Saiyaman: back to form 10
}
# map per scenario: world, map (the simulator arena with the same map)
MAPS = {'htc': (6, 9), 'crater': (8, 71)}


def keymask(s):
    if isinstance(s, int):
        return s
    m = 0
    for k in s.split('+'):
        if k:
            m |= KEYS[k]
    return m


def form_char(form):
    return u8(FORMS + 24 * form + 0xC)


def state_name(e, obj):
    if not obj:
        return 'none'
    vt = e.r32(obj)
    return STATE_NAMES.get(vt, 'vt_%08X' % vt)


def anim_raw(e, p):
    d = e.r32(p + 0x58)
    a = e.r32(p + 0x50)
    n = e.r8(d)
    for r in range(n + 2):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def setup(scn):
    """Emulator at frame 0 of the scenario: the form's player standing idle in the Hyperbolic Time Chamber (6-9)."""
    e = emu('buu_snakeway_start.ss')
    form = scn['form']
    ci = form_char(form)
    rec = REC0 + 0x68 * ci
    e.w8(GD, ci)
    e.w8(rec + 0x14, form)
    st = scn.get('stats', [40, 40, 40, 40])
    for i, v in enumerate(st):
        e.w8(rec + 0x28 + i, v)
    for i, v in enumerate([st[0], st[1], st[2], st[2], st[3]]):
        e.w16(rec + 0x4C + 2 * i, v)
    hp, ep = scn.get('hp', 900), scn.get('ep', 900)
    e.w16(rec + 0x16, hp)
    e.w16(rec + 0x18, scn.get('maxhp', max(hp, 900)))
    e.w16(rec + 0x1A, ep)
    e.w16(rec + 0x1C, scn.get('maxep', max(ep, 900)))
    e.w32(rec + 0x04, 0)  # SS cooldown
    # SS / SS3 gauges full (a transformed form with an empty gauge reverts on the first frame)
    e.w16(rec + 0x1E, scn.get('ss', max(ep, 900)))
    e.w16(rec + 0x20, scn.get('ss3', max(ep, 900)))
    sk = scn.get('skills')
    if sk is not None:
        e.w8(rec + 0x0C, len(sk))
        for i in range(7):
            e.w8(rec + 0x0D + i, sk[i] if i < len(sk) else 0)
        e.w8(rec + 0x15, scn.get('slot', 0))
        for i in range(len(sk)):
            e.w8(rec + 0x2E + 2 * i, scn.get('skill_level', 0))
            e.w8(rec + 0x2F + 2 * i, scn.get('skill_level', 0))
    wm = MAPS[scn.get('arena', 'htc')]
    ok = warp(e, wm[0], wm[1], 104, 152, frames=240)
    assert ok, 'warp failed'
    p = e.r32(PLAYER)
    x, y = scn.get('pos', [160, 150])
    e.w32(p + 0x1A0, x << 8)
    e.w32(p + 0x1A4, y << 8)
    e.w8(p + 0x10, scn.get('dir', 0))
    e.set_keys(0)
    e.run(scn.get('settle', 30))
    return e


def snapshot(e):
    p = e.r32(PLAYER)
    ci = e.r8(GD)
    rec = REC0 + 0x68 * ci
    return p, rec


def row(e):
    p, rec = snapshot(e)
    st = e.r32(p + 0x14)
    raw = anim_raw(e, p)
    fr = e.r8(p + 0x11)
    d = e.r8(p + 0x10)
    a = e.r32(p + 0x50)
    nfr = e.r32(a) if a else 0
    rom = e.r32(a + 4 + 16 * fr + 4 * d) if a and fr < max(nfr, 1) and nfr < 64 else 0
    return {
        'x': e.r32(p + 0x1A0), 'y': e.r32(p + 0x1A4), 'dir': d, 'raw': raw, 'frame': fr, 'tick': e.r16(p + 0x12),
        'state': state_name(e, st), 'rom': rom, 'form': e.r8(rec + 0x14), 'hp': struct.unpack('<h', struct.pack('<H', e.r16(rec + 0x16)))[0],
        'ep': struct.unpack('<h', struct.pack('<H', e.r16(rec + 0x1A)))[0],
        'rng': e.r32(RNG),
    }


def write_hit(e, hit):
    """message kind 0 (attack) aimed at the player's body box: {box, dir, mult, attack type, attacker}"""
    p = e.r32(PLAYER)
    for i in range(0, 0x200, 4):
        e.w32(ATTACKER + i, 0)
    for i in range(0, 0x60, 4):
        e.w32(FAKE_VT + i, (BX_LR - FAKE_VT) & 0xFFFFFFFF)
    e.w32(ATTACKER, FAKE_VT)
    e.w32(ATTACKER + 0x1A0, e.r32(p + 0x1A0) + (hit.get('dx', 32) << 8))
    e.w32(ATTACKER + 0x1A4, e.r32(p + 0x1A4))
    e.w16(ATTACKER + 0x1B0, hit.get('str', 30))
    e.w16(ATTACKER + 0x1B2, hit.get('pow', 30))
    px, py = e.r32(p + 0x1A0) >> 8, e.r32(p + 0x1A4) >> 8
    for i in range(0, 0x40, 4):
        e.w32(MSG + i, 0)
    e.w8(MSG, 0)
    for i, v in enumerate([px - 32, py - 64, px + 48, py + 32]):
        e.w32(MSG + 4 + 4 * i, v & 0xFFFFFFFF)
    e.w32(MSG + 0x14, hit.get('dir', 2))
    e.w32(MSG + 0x18, hit.get('mult', 0x400))
    e.w32(MSG + 0x1C, ATK_TABLE + 8 * hit.get('atk', 2))
    e.w32(MSG + 0x20, ATTACKER)
    e.w32(MSG + 0x30, ATTACKER)


class SoundLog:
    def __init__(self, e):
        self.e = e
        self.ids = [e.breakpoint(a) for a in (PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG)]

    def collect(self):
        out = []
        for h in self.e.hits():
            if h['kind'] != 'break':
                continue
            a = h['addr'] & ~1
            r = h['regs']
            if a == PLAY_SFX:
                out.append(['play', (r[1] - SFX_TABLE) // 12, r[2] & 0xFF])
            elif a == STOP_SFX:
                out.append(['stop', (r[1] - SFX_TABLE) // 12])
            elif a == STOP_ALL:
                out.append(['stopall'])
            elif a == STOP_MATCH:
                out.append(['stopmatch', r[1]])
            elif a == PLAY_SONG:
                out.append(['song', (r[1] - SONG_TABLE) // 20 if SONG_TABLE <= r[1] < SONG_TABLE + 53 * 20 else -1])
        return out


def record(scn):
    """-> trace dict (self-contained: setup values the simulator copies + per-frame expectations)"""
    e = setup(scn)
    p, rec = snapshot(e)
    init = row(e)
    init['rng'] = [e.r32(RNG), e.r32(RNG + 4)]
    init['rec'] = {
        'char': e.r8(GD), 'form': e.r8(rec + 0x14), 'hp': e.r16(rec + 0x16), 'maxHp': e.r16(rec + 0x18), 'ep': e.r16(rec + 0x1A),
        'maxEp': e.r16(rec + 0x1C), 'base': [e.r8(rec + 0x28 + i) for i in range(4)], 'eff': [e.r16(rec + 0x4C + 2 * i) for i in range(5)],
        'skills': [e.r8(rec + 0x0D + i) for i in range(e.r8(rec + 0x0C))], 'slot': e.r8(rec + 0x15),
        'levels': [e.r8(rec + 0x2E + 2 * i) for i in range(e.r8(rec + 0x0C))], 'ssGauge': e.r16(rec + 0x1E), 'ss3Gauge': e.r16(rec + 0x20),
        'epCost': e.r8(rec + 0x56), 'epBlock': e.r8(rec + 0x57), 'blockFactor': e.r16(rec + 0x60), 'ssRate': e.r16(rec + 0x62),
        'equip': [e.r32(rec + 0x3C + 4 * i) for i in range(4)],
    }
    ctl = e.r32(p + 0x22C)  # the form's per-frame controller object: +0xC EP regen accumulator, +0x14 SS/SS3 drain acc
    init['obj'] = {'walk': e.r32(p + 0x238), 'dash': e.r32(p + 0x23C), 'stats': [e.r16(p + 0x1B0 + 2 * i) for i in range(4)],
                   'resist': [e.r8(p + 0x1B8), e.r8(p + 0x1B9)], 'foot': [e.r8(p + 0x19C), e.r8(p + 0x19D)],
                   'regen': e.r32(ctl + 0xC), 'ssacc': e.r32(ctl + 0x14), 'cooldown': e.r32(rec + 4),
                   'idlecnt': e.r32(0x03002B48),  # the idle state's frame counter (1024 -> bored animation)
                   'camera': [e.r32(e.r32(0x080707E8)), e.r32(e.r32(0x080707E8) + 4)]}  # 22.10 camera (0x080077EC)
    # the HUD's low-HP beep timestamp (HUD object +0x10 vs the frame counter 0x03000F94; 0x0800BA4A): peek one frame
    st = e.save()
    bp = e.breakpoint(0x0800BA06)
    e.run(1)
    hud = [h['regs'][4] for h in e.hits() if h['kind'] == 'break']
    e.clear(bp)
    e.load(st)
    if hud:
        init['obj']['beepAge'] = (e.r32(0x03000F94) - e.r32(hud[0] + 0x10)) & 0xFFFFFFFF
    snd = SoundLog(e)
    hits = {h['at']: h for h in scn.get('hits', [])}
    frames = []
    inputs = [[keymask(k), n] for k, n in scn['input']]
    f = 0
    for keys, n in inputs:
        for _ in range(n):
            f += 1
            e.set_keys(keys)
            if f in hits:
                write_hit(e, hits[f])
                call(e, PLAYER_MSG, (e.r32(PLAYER), 0, MSG, 0))
            else:
                e.run(1)
            r = row(e)
            s = snd.collect()
            if s:
                r['sfx'] = s
            frames.append(r)
    e.set_keys(0)
    return {'name': scn['name'], 'scenario': scn, 'input': inputs, 'init': init, 'frames': frames}
