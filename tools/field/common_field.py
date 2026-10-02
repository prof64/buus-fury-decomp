"""Field-game ground truth: shared emulator helpers for the field recorders (headless, muted).

Addresses (Buu's Fury USA, see re/buu/notes.md "Field: area loading and transitions"):
  0x0300197C  world state: +0 u8 world, +1 u8 area, +2 u8 variant, +4 area record ptr, +8 u16 current song,
              +0xC u8 "area has +8 byte" flag, +0x10 u32 area-load counter
  0x03001C4C  object world: +4 u32 count + object ptrs (update order: last to first), +0x610 deferred deletes
  0x03002A64  trigger list: u32 count + trigger ptrs (+0xC4 check counter, +0xD0 enabled)
  0x03001C34  player object ptr;  0x03001068 GameData (+0 character, +0x55 story flags, +0x8D map-visited bits)
  0x03001560 + 0x68*i  character records;  0x03004A5C RNG s0, s1;  0x03000F94 logic frame counter
  [0x080707E8] camera (22.10 x, y at +0 / +4)
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import emu, KEY, u8, u16, u32, s32, STATES  # noqa: E402,F401
from emutools import cur_pc  # noqa: E402

WS = 0x0300197C
OBJW = 0x03001C4C
TRIGS = 0x03002A64
PLAYER = 0x03001C34
GD = 0x03001068
REC0 = 0x03001560
RNG = 0x03004A5C
FC = 0x03000F94
CAM_PTR = 0x080707E8
ENTER_AREA = 0x08008D44
FLAG_BYTES = 0x80  # GD+0x55 .. +0xD5 (the condition word addresses flags < 0x400)

SFX_TABLE = 0x083192A8
SONG_TABLE = 0x083BB78C
PLAY_SFX = 0x0803E378
STOP_SFX = 0x0803E302
STOP_ALL = 0x0803E2EC
STOP_MATCH = 0x0803E32C
PLAY_SONG = 0x0803F110
RAND, CHANCE, NEXT = 0x0803FACC, 0x0803FADE, 0x0803FAAC
ENEMY_LIST = 0x03001BB0
AI_STATES = 0x0870F514
PATH_STAGGER = 0x030032E5
FADE_OUT_STEP = 0x08007132  # fade-out mode +0x28: object update, brightness -= 8, done at <= 0 (32 frames)
FADE_IN_STEP = 0x080070E4   # fade-in mode +0x28: object update, brightness += 8, done at >= 256 (32 frames)

# object vtables (W+4 list); actors carry x/y at +0x1A0/+0x1A4 and facing at +0x10
VT = {
    0x0804C9F0: 'player', 0x0804C484: 'enemy', 0x0804C92C: 'npc', 0x0804CEF4: 'npc_talk', 0x0804CE00: 'breakable',
    0x08049FEC: 'hud',
    # triggers (trigger list)
    0x0804A09C: 'exit', 0x08049BC4: 'edge_exit', 0x08049BD4: 'event_trigger', 0x08049784: 'script_trigger',
    0x08049794: 'save_point', 0x0804B070: 'message_trigger', 0x080499D4: 'region',
}
ACTORS = {'player', 'enemy', 'npc', 'npc_talk'}
STATE_NAMES = {
    0x0804C790: 'idle', 0x0804A77C: 'idle', 0x0804C3A8: 'walk', 0x0804BEF0: 'dash', 0x0804E00C: 'autowalk',
    0x0804C384: 'bored', 0x0804C35C: 'punch', 0x0804A188: 'hold0', 0x0804985C: 'hold', 0x0804D784: 'once',
    0x0804C744: 'block', 0x0804D75C: 'hurt', 0x0804C768: 'ko',
}


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def s16v(v):
    return v - 0x10000 if v & 0x8000 else v


def area(e):
    return [e.r8(WS), e.r8(WS + 1), e.r8(WS + 2)]


def rng(e):
    return [e.r32(RNG), e.r32(RNG + 4)]


def camera(e):
    c = e.r32(CAM_PTR)
    return [s32v(e.r32(c)), s32v(e.r32(c + 4))]


def flags_hex(e):
    return e.read(GD + 0x55, FLAG_BYTES).hex()


def state_name(e, p):
    st = e.r32(p + 0x14)
    if not st:
        return 'none'
    vt = e.r32(st)
    return STATE_NAMES.get(vt, 'vt_%08X' % vt)


def player_row(e):
    p = e.r32(PLAYER)
    if not p:
        return None
    return [s32v(e.r32(p + 0x1A0)), s32v(e.r32(p + 0x1A4)), e.r8(p + 0x10), state_name(e, p)]


def record_of(e):
    ci = e.r8(GD)
    rec = REC0 + 0x68 * ci
    n = e.r8(rec + 0x0C)
    return {
        'char': ci, 'form': e.r8(rec + 0x14), 'hp': e.r16(rec + 0x16), 'maxhp': e.r16(rec + 0x18), 'ep': e.r16(rec + 0x1A),
        'maxep': e.r16(rec + 0x1C), 'str': e.r8(rec + 0x28), 'pow': e.r8(rec + 0x29), 'end': e.r8(rec + 0x2A),
        'spd': e.r8(rec + 0x2B), 'points': e.r16(rec + 0x2C), 'xp': e.r32(rec + 0x24),
        'skills': [e.r8(rec + 0x0D + i) for i in range(n)], 'slot': e.r8(rec + 0x15),
        'skill_levels': [e.r8(rec + 0x2E + 2 * i) for i in range(n)],
        'equip': [s32v(e.r32(rec + 0x3C + 4 * i)) for i in range(4)],
    }


def objs(e):
    """W+4 object list in list order (index 0 = oldest; the world updates last to first):
    [kind, vtable, x_px, y_px, facing] (x/y/facing only for actors, else None)"""
    out = []
    n = e.r32(OBJW + 4)
    for k in range(min(n, 255)):
        o = e.r32(OBJW + 8 + 4 * k)
        vt = e.r32(o)
        kind = VT.get(vt, 'obj')
        if kind in ACTORS:
            out.append([kind, '%08X' % vt, s32v(e.r32(o + 0x1A0)) >> 8, s32v(e.r32(o + 0x1A4)) >> 8, e.r8(o + 0x10)])
        else:
            out.append([kind, '%08X' % vt, None, None, None])
    return out


def enemy_detail(e):
    """enemy list 0x03001BB0 in order: {obj, type, x, y, facing, hp1024, stack: [[state, node_obj, node_vt]...],
    mark1 (+0x1E8), mark2 (+0x1EC), stamp (+0x210), ctrl (+0x14 vtable), tick (+0x12), anim frame (+0x11)}"""
    out = []
    n = e.r32(ENEMY_LIST)
    for k in range(min(n, 64)):
        o = e.r32(ENEMY_LIST + 4 + 4 * k)
        st = o + 0x238
        depth = e.r32(st + 4)
        stack = []
        for i in range(min(depth, 8)):
            node = e.r32(st + 8 + 8 * i)
            sdef = e.r32(st + 12 + 8 * i)
            stack.append([(sdef - AI_STATES) // 8, '%08X' % node, '%08X' % (e.r32(node) if node else 0)])
        ctl = e.r32(o + 0x14)
        out.append({'obj': '%08X' % o, 'type': (e.r32(o + 0x224) - 0x08711274) // 28, 'x': s32v(e.r32(o + 0x1A0)),
                    'y': s32v(e.r32(o + 0x1A4)), 'facing': e.r8(o + 0x10), 'hp1024': s32v(e.r32(o + 0x28C)), 'stack': stack,
                    'mark1': e.r32(o + 0x1E8), 'mark2': e.r32(o + 0x1EC), 'stamp': e.r32(o + 0x210), 'v1e4': s32v(e.r32(o + 0x1E4)),
                    'ctrl': '%08X' % (e.r32(ctl) if ctl else 0), 'tick': e.r16(o + 0x12), 'aframe': e.r8(o + 0x11)})
    return out


def trigs(e):
    """trigger list (count + ptrs): [kind, vtable, x0, y0, x1, y1]"""
    out = []
    n = e.r32(TRIGS)
    for k in range(min(n, 255)):
        o = e.r32(TRIGS + 4 + 4 * k)
        vt = e.r32(o)
        w1, w2 = e.r32(o + 4), e.r32(o + 8)
        out.append([VT.get(vt, 'trigger'), '%08X' % vt, s16v(w1 & 0xFFFF), s16v(w1 >> 16), s16v(w2 & 0xFFFF), s16v(w2 >> 16)])
    return out


class SoundLog:
    """PlaySfx / StopSfx / StopAll / StopMatch / PlaySong calls (breakpoints, collected per frame)"""
    def __init__(self, e):
        self.e = e
        self.ids = [e.breakpoint(a) for a in (PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG, FADE_OUT_STEP, FADE_IN_STEP,
                                               RAND, CHANCE, NEXT)]

    def draws(self, hits):
        """RNG calls of the frame in call order: [caller lr, 'rand'|'chance'|'next', arg (None for next), r4, r5, r6, r7]
        (inner next() of rand / chance omitted)"""
        out = []
        for h in hits:
            if h['kind'] != 'break':
                continue
            a = h['addr'] & ~1
            name = {RAND: 'rand', CHANCE: 'chance', NEXT: 'next'}.get(a)
            if name is None:
                continue
            r = h['regs']
            lr = r[14] & ~1
            if name == 'next' and RAND <= lr < CHANCE + 0x20:
                continue
            out.append(['%08X' % lr, name, r[0] if name != 'next' else None] + ['%08X' % (r[k] & 0xFFFFFFFF) for k in (4, 5, 6, 7)])
        return out

    def collect(self, hits):
        out = []
        for h in hits:
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
                out.append(['stopmatch', (r[1] - SFX_TABLE) // 12])
            elif a == PLAY_SONG:
                out.append(['song', (r[1] - SONG_TABLE) // 20 if SONG_TABLE <= r[1] < SONG_TABLE + 53 * 20 else -1])
        return out

    def close(self):  # noqa
        for i in self.ids:
            self.e.clear(i)


def load_info(e):
    """at the first instruction of EnterArea(ws, world, area, &dest, [sp] variant, [sp+4] flag)"""
    sp = e.reg(13)
    dest = e.reg(3)
    return {'to': [e.reg(1) & 0xFF, e.reg(2) & 0xFF], 'variant_arg': e.r32(sp) & 0xFF, 'flag': e.r32(sp + 4),
            'dest': [s16v(e.r16(dest)), s16v(e.r16(dest + 2))], 'lr': e.reg(14) & ~1,
            'rng': rng(e), 'flags_hex': flags_hex(e), 'from': area(e), 'record': record_of(e), 'fc': e.r32(FC),
            'stagger': e.r8(PATH_STAGGER)}


def run_frame(e):
    """run one video frame; if EnterArea runs in it, return its arguments + state at entry (replays the frame
    instruction by instruction from a save state up to the call)"""
    st = e.save()
    bp = e.breakpoint(ENTER_AREA)
    e.run(1)
    hs = e.hits()
    e.clear(bp)
    if not any(h['kind'] == 'break' and (h['addr'] & ~1) == ENTER_AREA for h in hs):
        return None, hs
    e.load(st)
    start = e.frame
    info = None
    while e.frame == start:
        if info is None and cur_pc(e) == ENTER_AREA:
            info = load_info(e)
        e.step()
    e.hits()  # the replay re-hits the breakpoints: keep the first run's hits
    return info, hs


# --- warp: the Snakeway-start text trigger converted into an edge exit (re/buu/warp.py), used to *arrive* somewhere
SNAKE_TRIG = 0x0203DFA4
EXIT_DATA = 0x0203FF40


def arm_warp(e, w, m, x, y, variant=0):
    """convert the Snakeway text trigger into an edge exit to (w, m) whose relative destination is exactly (x, y):
    the source anchor follows the player each frame (call follow_warp before every frame)"""
    assert e.r32(SNAKE_TRIG) == 0x08049BD4, hex(e.r32(SNAKE_TRIG))
    for i in range(0, 0x20, 4):
        e.w32(EXIT_DATA + i, 0)
    e.w8(EXIT_DATA + 0x10, 3)  # push / facing: right
    e.w8(EXIT_DATA + 0x11, w)
    e.w8(EXIT_DATA + 0x12, m)
    e.w8(EXIT_DATA + 0x13, variant)
    e.w16(EXIT_DATA + 0x14, x)
    e.w16(EXIT_DATA + 0x16, y)
    e.w32(SNAKE_TRIG, 0x08049BC4)
    e.w8(SNAKE_TRIG + 0x10, 3)
    e.w8(SNAKE_TRIG + 0x11, 0xFF)  # any held key
    e.w32(SNAKE_TRIG + 0x14, EXIT_DATA)
    p = e.r32(PLAYER)
    e.w32(p + 0x1A0, 0x160 << 8)
    e.w32(p + 0x1A4, 0xA0 << 8)


def follow_warp(e):
    p = e.r32(PLAYER)
    if p and e.r32(SNAKE_TRIG) == 0x08049BC4:
        e.w16(EXIT_DATA + 8, e.r32(p + 0x1A0) >> 8)
        e.w16(EXIT_DATA + 0xA, e.r32(p + 0x1A4) >> 8)


def record(name, source_state, setup, inputs, notes='', max_setup=600, frame0_load=1):
    """Run `setup(e)` (returns per-frame pre-hook or None) with RIGHT held until the `frame0_load`-th area load,
    then record from the first logic frame after that load: frame 0 = the first video frame after EnterArea
    returned in which the logic frame counter advanced. `inputs` = [[keys, video_frames], ...] from frame 0."""
    e = emu(source_state)
    pre = setup(e)
    loads = 0
    init_load = None
    e.set_keys(KEY.RIGHT)
    for _ in range(max_setup):
        if pre:
            pre(e)
        info, _ = run_frame(e)
        if info:
            loads += 1
            if loads == frame0_load:
                init_load = info
                break
    assert init_load, 'no area load during setup'
    # wait for the first logic frame of the new area: peek frame by frame (no input: the game ignores it here)
    e.set_keys(0)
    pending = []
    while True:
        st = e.save()
        fc = e.r32(FC)
        e.run(1)
        if e.r32(FC) != fc:
            e.load(st)
            break
        pending.append({'player': player_row(e), 'rng': rng(e)})
    p = e.r32(PLAYER)
    init = {
        'area': area(e), 'rng': init_load['rng'], 'flags_hex': init_load['flags_hex'],
        'rng_frame0': rng(e),  # state after the load + its non-logic frames (= what frame 0 starts from)
        'player': {'x': s32v(e.r32(p + 0x1A0)), 'y': s32v(e.r32(p + 0x1A4)), 'facing': e.r8(p + 0x10),
                   'state': state_name(e, p), 'form': init_load['record']['form'], 'record': init_load['record']},
        'entry': {'kind': 'edge_exit', 'warp': True, 'to': init_load['to'], 'variant_arg': init_load['variant_arg'],
                  'dest': init_load['dest'], 'from_area': init_load['from'], 'enter_area_flag': init_load['flag']},
        'camera': camera(e), 'zenie': e.r32(0x0300155C), 'nonlogic_frames_before_frame0': len(pending),
        'objs': objs(e), 'trigs': trigs(e),
        'fc': e.r32(FC), 'fc_load': init_load['fc'], 'path_stagger': e.r8(PATH_STAGGER),
        'path_stagger_load': init_load['stagger'], 'enemies': enemy_detail(e),
    }
    init['rng_draws_load'] = rng_steps(init['rng'], init['rng_frame0'], 5000)
    snd = SoundLog(e)
    frames = []
    sfx = []
    loads = []
    f = 0
    fade = [None, 0]
    for keys, n in inputs:
        for _ in range(n):
            e.set_keys(keys)
            fc = e.r32(FC)
            info, hs = run_frame(e)
            row = {'f': f, 'logic': e.r32(FC) != fc, 'area': area(e), 'p': player_row(e), 'rng': rng(e),
                   'cam': camera(e), 'fc': e.r32(FC), 'stagger': e.r8(PATH_STAGGER)}
            row['draws'] = snd.draws(hs)
            row['enemies'] = enemy_detail(e)
            # phase: 'normal', ['fade_out', k] / ['fade_in', k] (k = 1..32: the fade mode's k-th logic frame),
            # 'load' (no logic: loading / waiting for the first fade-in frame)
            kinds = {h['addr'] & ~1 for h in hs if h['kind'] == 'break'}
            if FADE_OUT_STEP in kinds or FADE_IN_STEP in kinds:
                kind = 'fade_out' if FADE_OUT_STEP in kinds else 'fade_in'
                fade = [kind, fade[1] + 1 if fade[0] == kind else 1]
                row['phase'] = list(fade)
            else:
                fade = [None, 0]
                row['phase'] = 'normal' if row['logic'] else 'load'
            row['objs'] = objs(e)
            if info:
                info['f'] = f
                loads.append(info)
                row['load'] = {k: info[k] for k in ('to', 'variant_arg', 'dest', 'rng', 'flag')}
            if loads and loads[-1]['f'] == f - 1 or f == 0:
                row['trigs'] = trigs(e)
            # first frame with a new player object after a load: dump the triggers too
            if frames and frames[-1]['p'] is None and row['p'] is not None:
                row['trigs'] = trigs(e)
            for s in snd.collect(hs):
                sfx.append([f] + s)
            frames.append(row)
            f += 1
    snd.close()
    e.set_keys(0)
    # per load: the first video frame with the new player, the first logic frame, the RNG the load left
    for L in loads:
        j = L['f'] + 1
        while j < len(frames) and frames[j]['p'] is None:
            j += 1
        k = j
        while k < len(frames) and not frames[k]['logic']:
            k += 1
        if k < len(frames):
            L['first_player_frame'] = j
            L['first_logic_frame'] = k
            L['nonlogic_frames'] = k - L['f'] - 1
            L['rng_after_load'] = frames[k - 1]['rng']
            L['rng_draws'] = rng_steps(L['rng'], L['rng_after_load'], 5000)
    return {'name': name, 'source_state': source_state, 'notes': notes, 'init': init,
            'input': [[k, n] for k, n in inputs], 'frames': frames, 'sfx': sfx,
            'loads': [{k: v for k, v in L.items() if k != 'record'} for L in loads]}


def rng_next(s):
    s0, s1 = s
    a = ((s0 + 0x02CBBC71) & 0xFFFFFFFF) ^ s1
    r = ((s1 << 3) | (s1 >> 29)) & 0xFFFFFFFF
    b = ((r + 0x0632D80F) & 0xFFFFFFFF) ^ a
    return [a, b]


def rng_steps(a, b, limit=100000):
    """number of next() calls from state a to state b (None if not reached within limit)"""
    s = list(a)
    for n in range(limit):
        if s == list(b):
            return n
        s = rng_next(s)
    return None
