"""Script / cutscene lockstep recorder (ground truth for wf-core's scene systems and wf-script-core's host).

Runs bytecode (a micro scenario) or a whole event graph through the game's own runners from a save state and
records, for every frame, what the cutscene systems show:

  actors   every world object with a sprite def (obj+0x58 maps to a script sprite id through 0x08000158):
           [obj#, sprite, x, y, z (24.8), dir (+0x10), raw anim (def+0x50 table index of +0x50), frame (+0x11)]
           obj# = stable index of the object pointer in `objs` (first appearance order); the player is flagged.
  cam      camera 0x030019A4 +0 / +4 (22.10)
  io       DISPCNT, BLDCNT, BLDALPHA, BLDY, MOSAIC (as the bus reads them) + palette checksums (BG, OBJ)
  snd      PlaySfx / StopSfx / StopAll / StopMatch / PlaySong calls (ids as in the sound tables)
  steps    scene step visits [scene, state, kind, arg, next | exit value, text ref]
  hud      GameData byte 1 (bit 1 HUD shown)
Frame 0 is the frame in which the call starts (the runner is entered at the start of that frame's game update,
like a scene started by a trigger); `done` is the frame in which the runner returned.

Usage (library): rec = record_code(state, asm([...]), frames=..); rec = record_event(state, event, params, ...)
Output files: data/script_lockstep/<name>.json (see scenarios.py)."""
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, u8, u16, u32, s32, KEY, WORK, STATES  # noqa: E402
from script_nat_b_lib import asm, vint, start_call, CODE_AT, HDR_AT, INTERP_AT  # noqa: E402,F401
from emutools import MAILBOX  # noqa: E402

OUT = ROOT / 'data' / 'script_lockstep'
WORLD = 0x03001C4C
PLAYER = 0x03001C34
CAM = 0x030019A4
GD = 0x03001068
SPRITE_DEFS = 0x086B6BB4      # def(id) = u32[SPRITE_DEFS + 4 * id] for id >= 10 (0x08000158)
SCRIPTS = 0x0879B704
SCENE_RUN, SCENE_STEP, RUNNER = 0x08012588, 0x08012538, 0x08004098
PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG = 0x0803E378, 0x0803E302, 0x0803E2EC, 0x0803E32C, 0x0803F110
SFX_TABLE, SONG_TABLE = 0x083192A8, 0x083BB78C
PARAMS_AT = 0x0203EFC0

_defs = None


def sprite_defs():
    """sprite def pointer -> script sprite id (ids 10..400; the first id wins for shared defs)"""
    global _defs
    if _defs is None:
        _defs = {}
        for i in range(10, 400):
            d = u32(SPRITE_DEFS + 4 * i)
            if 0x08000000 <= d < 0x0A000000:
                _defs.setdefault(d, i)
    return _defs


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def world_objects(e):
    n = e.r32(WORLD + 4)
    out = [e.r32(WORLD + 8 + 4 * i) for i in range(min(n, 256))]
    return out


def anim_raw(e, o):
    d = e.r32(o + 0x58)
    a = e.r32(o + 0x50)
    if not a:
        return -1
    n = e.r8(d)
    for r in range(n + 2):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def palsum(e, base, n=0x200):
    b = e.read(base, n)
    return sum(int.from_bytes(b[i:i + 2], 'little') for i in range(0, n, 2)) & 0xFFFFFF


class Recorder:
    def __init__(self, e):
        self.e = e
        self.objs = []          # [ptr, vtable, sprite] in first-appearance order
        self.index = {}         # (ptr, vt, def) -> obj#
        self.frames = []
        self.bps = []

    def arm(self):
        e = self.e
        self.bps = [e.breakpoint(a) for a in (PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG, SCENE_STEP, SCENE_RUN)]

    def disarm(self):
        for b in self.bps:
            self.e.clear(b)
        self.bps = []

    def obj_id(self, o, vt, d):
        k = (o, vt, d)
        if k not in self.index:
            self.index[k] = len(self.objs)
            self.objs.append({'ptr': hex(o), 'vt': hex(vt), 'sprite': sprite_defs().get(d, -1),
                              'player': o == self.e.r32(PLAYER)})
        return self.index[k]

    def actors(self):
        """actor rows [obj#, sprite, x, y, z, dir, raw, frame, tick]; non-actor objects go to self.others"""
        e = self.e
        defs = sprite_defs()
        out = []
        self.others = []
        for o in world_objects(e):
            if not (0x02000000 <= o < 0x04000000):
                continue
            d = e.r32(o + 0x58)
            vt = e.r32(o)
            if d not in defs:
                self.others.append(self.obj_id(o, vt, 0))
                continue
            k = self.obj_id(o, vt, d)
            out.append([k, defs[d], s32v(e.r32(o + 0x1A0)), s32v(e.r32(o + 0x1A4)), s32v(e.r32(o + 0x1A8)),
                        e.r8(o + 0x10), anim_raw(e, o), e.r8(o + 0x11), e.r16(o + 0x12)])
        return out

    def hits(self):
        snd, steps = [], []
        for h in self.e.hits():
            if h['kind'] != 'break':
                continue
            a = h['addr'] & ~1
            r = h['regs']
            if a == PLAY_SFX:
                snd.append(['play', (r[1] - SFX_TABLE) // 12, r[2] & 0xFF])
            elif a == STOP_SFX:
                snd.append(['stop', (r[1] - SFX_TABLE) // 12])
            elif a == STOP_ALL:
                snd.append(['stopall'])
            elif a == STOP_MATCH:
                snd.append(['stopmatch', (r[1] - SFX_TABLE) // 12])
            elif a == PLAY_SONG:
                snd.append(['song', (r[1] - SONG_TABLE) // 20 if SONG_TABLE <= r[1] < SONG_TABLE + 53 * 20 else -1])
            elif a == SCENE_RUN:
                steps.append(['run', header_event(r[0])])
            elif a == SCENE_STEP:
                w = r[1]
                if w >= 0x10000:
                    kind = u8(w + 2)
                    ref = u32(w + 4) if kind in (5, 6) else 0
                    steps.append(['step', u16(w), kind, u8(w + 3), hex(w), ref])
                else:
                    steps.append(['exit', w])
        return snd, steps

    def row(self, keys):
        e = self.e
        snd, steps = self.hits()
        acts = self.actors()
        return {
            'keys': keys,
            'actors': acts,
            'others': self.others,
            'cam': [s32v(e.r32(CAM)), s32v(e.r32(CAM + 4))],
            'io': [e.r16(0x04000000), e.r16(0x04000050), e.r16(0x04000052), e.r16(0x04000054), e.r16(0x0400004C),
                   palsum(e, 0x05000000), palsum(e, 0x05000200)],
            'hud': e.r8(GD + 1),
            'lf': e.r32(0x03000F94),
            'rng': [e.r32(0x03004A5C), e.r32(0x03004A60)],
            'snd': snd,
            'steps': steps,
        }


_hdr = None


def header_event(h):
    global _hdr
    if _hdr is None:
        _hdr = {}
        for i in range(522):
            _hdr.setdefault(u32(SCRIPTS + 4 * i), i)
    return _hdr.get(h, -1)


def snapshot(e):
    """initial state the replay builds its World from: actors, camera, area, GameData 0x03001068 (0x600 bytes:
    char +0, supplies +2.., flags +0x55.., equipment list +0xE4, zenie +0x4F4), the 9 character records
    0x03001560 (0x68 each), script words G 0x03001058 and L 0x03001020, the map size (area manager)"""
    r = Recorder(e)
    acts = r.actors()
    return {'objs': r.objs, 'actors': acts, 'cam': [s32v(e.r32(CAM)), s32v(e.r32(CAM + 4))],
            'area': [e.r8(0x0300197C + k) for k in range(3)],
            'rng': [e.r32(0x03004A5C), e.r32(0x03004A60)], 'lf': e.r32(0x03000F94), 'song': ((e.r32(0x030038A4 + 0xC) - SONG_TABLE) // 20) if e.r8(0x030038A4 + 5) == 1 else -1,
            'gd': e.read(GD, 0x600).hex(), 'recs': e.read(0x03001560, 0x68 * 9).hex(),
            'g': [s32v(e.r32(0x03001058 + 4 * i)) for i in range(4)],
            'l': [s32v(e.r32(0x03001020 + 4 * i)) for i in range(4)],
            'char': e.r8(GD), 'player': hex(e.r32(PLAYER)), 'others': r.others}, r


def first_scene_frame(state, frames, keys=None, pre=None, settle=3):
    """frame index (0-based, as in record_natural) of the first run_scene call"""
    e = emu(state)
    e.run(settle)
    if pre:
        pre(e)
    b = e.breakpoint(SCENE_RUN)
    for f in range(frames):
        e.set_keys(keys(f) if keys else 0)
        e.run(1)
        if any(h['kind'] == 'break' for h in e.hits()):
            return f
    return None


def record_natural(state, frames, keys=None, pre=None, settle=3, after=None):
    """a scene started by the game itself (trigger, on-defeat callback, new-game hook): the recording starts at the
    frame in which run_scene is first called (frame 0, like record_event); `keys(f)` is indexed from the state.
    Returns (emu, doc) with doc['skip'] = frames run before the scene. Stops `after` frames after the start
    (default: `frames` total)."""
    f0 = first_scene_frame(state, frames, keys, pre, settle)
    if f0 is None:
        raise RuntimeError('no scene started')
    e = emu(state)
    e.run(settle)
    if pre:
        pre(e)
    for f in range(f0):
        e.set_keys(keys(f) if keys else 0)
        e.run(1)
    init, rec = snapshot(e)
    rec.arm()
    n = after or (frames - f0)
    for f in range(n):
        k = keys(f0 + f) if keys else 0
        e.set_keys(k)
        e.run(1)
        rec.frames.append(rec.row(k))
    rec.disarm()
    e.set_keys(0)
    ev = next((s[1] for s in rec.frames[0]['steps'] if s[0] == 'run'), None)
    return e, {'kind': 'natural', 'event': ev, 'skip': f0, 'init': init, 'objs': rec.objs, 'done': None,
               'frames': rec.frames}


def _run(e, rec, frames, keys, tail, until_done=True):
    done = None
    for f in range(frames):
        k = keys(f) if keys else 0
        e.set_keys(k)
        e.run(1)
        rec.frames.append(rec.row(k))
        if done is None and e.r32(MAILBOX) != 0xDEADBEEF:
            done = f
            if until_done:
                frames = min(frames, f + 1 + tail)
        if f + 1 >= frames:
            break
    e.set_keys(0)
    return done


def record_code(state, code, frames=600, keys=None, pre=None, tail=8, settle=3, params=0):
    """micro scenario: run bytecode `code` (bytes, ends with 0x11) through the runner 0x08004098"""
    e = emu(state)
    e.run(settle)
    if pre:
        pre(e)
    blob = b'\x01\x00\x00' + code
    blob += b'\x00' * (-len(blob) % 4)
    for i in range(0, len(blob), 4):
        e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
    e.w32(HDR_AT, CODE_AT)
    for i in range(0, 0x6c, 4):
        e.w32(INTERP_AT + i, 0)
    init, rec = snapshot(e)
    start_call(e, RUNNER, (INTERP_AT, HDR_AT, params, 0))
    rec.arm()
    done = _run(e, rec, frames, keys, tail)
    rec.disarm()
    return e, {'kind': 'code', 'code': code.hex(), 'init': init, 'objs': rec.objs, 'done': done, 'frames': rec.frames}


def record_event(state, event, params=(0, 0, 0, 0), frames=3000, keys=None, pre=None, tail=8, settle=3):
    """run event graph `event` through run_scene 0x08012588 (as an on-defeat callback / trigger would)"""
    e = emu(state) if not hasattr(state, 'r32') else state
    if not hasattr(state, 'r32'):
        e.run(settle)
    if pre:
        pre(e)
    for i, v in enumerate(params):
        e.w32(PARAMS_AT + 4 * i, v & 0xFFFFFFFF)
    init, rec = snapshot(e)
    start_call(e, SCENE_RUN, (u32(SCRIPTS + 4 * event), PARAMS_AT, 0, 0))
    rec.arm()
    done = _run(e, rec, frames, keys, tail)
    rec.disarm()
    return e, {'kind': 'event', 'event': event, 'params': list(params), 'init': init, 'objs': rec.objs, 'done': done,
               'frames': rec.frames}


def save(name, doc, meta=None):
    OUT.mkdir(parents=True, exist_ok=True)
    doc = dict(doc)
    doc['name'] = name
    doc['meta'] = meta or {}
    p = OUT / f"{name}.json"
    p.parent.mkdir(parents=True, exist_ok=True)
    # one frame per line: readable diffs, compact
    frames = doc.pop('frames')
    head = json.dumps(doc, separators=(',', ':'))
    body = ',\n'.join(json.dumps(f, separators=(',', ':')) for f in frames)
    p.write_text(head[:-1] + ',"frames":[\n' + body + '\n]}\n')
    return p


def mash(period=30, hold=4, key=KEY.A):
    return lambda f: key if f % period < hold else 0
