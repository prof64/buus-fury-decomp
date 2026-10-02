"""Helpers for the full cutscene scenarios (scenarios.FULL): scenes started by the game itself through paths other
than run_scene 0x08012588 (the new-game prologue runs scene #4 as a scene object stepped by the main loop), step
annotation (event / state of every step entry, from the ROM scene headers), setups (injected actors / lethal hits).

record_steps_natural(state, frames, keys, pre): like capture.record_natural, but the recording starts at the frame
of the first scene step (SCENE_STEP 0x08012538) or run_scene, whichever comes first."""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from capture import (emu, u8, u16, u32, s32v, snapshot, Recorder, SCENE_RUN, SCENE_STEP, SCRIPTS, KEY,  # noqa
                     header_event)
from emutools import call  # noqa: E402

_entries = None


def step_index():
    """step entry address -> (event, state) for every event header (first owner wins)"""
    global _entries
    if _entries is None:
        _entries = {}
        for ev in range(522):
            h = u32(SCRIPTS + 4 * ev)
            for st in range(256):
                w = u32(h + 4 * st)
                if w < 0x10000:
                    # terminals may sit between real states; keep scanning a little
                    continue
                if not (0x08000000 <= w < 0x09000000):
                    break
                _entries.setdefault(w, (ev, st))
    return _entries


def annotate(doc):
    """add [event, state] to every recorded 'step' row (entry address -> owner)"""
    idx = step_index()
    for f in doc['frames']:
        for s in f['steps']:
            if s[0] == 'step':
                ev, st = idx.get(int(s[4], 16), (-1, -1))
                s.extend([ev, st])
    return doc


def first_step_frame(state, frames, keys=None, pre=None, settle=3):
    e = emu(state)
    e.run(settle)
    if pre:
        pre(e)
    bs = [e.breakpoint(SCENE_STEP), e.breakpoint(SCENE_RUN)]
    for f in range(frames):
        e.set_keys(keys(f) if keys else 0)
        e.run(1)
        if any(h['kind'] == 'break' for h in e.hits()):
            return f
    return None


def record_steps_natural(state, frames, keys=None, pre=None, settle=3, after=None, stop=None):
    """stop(doc_frames) -> True ends the recording early (checked every frame)"""
    f0 = first_step_frame(state, frames, keys, pre, settle)
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
        if stop and stop(rec.frames):
            break
    rec.disarm()
    e.set_keys(0)
    doc = {'kind': 'natural', 'event': None, 'skip': f0, 'init': init, 'objs': rec.objs, 'done': None,
           'frames': rec.frames}
    annotate(doc)
    for s in doc['frames'][0]['steps']:
        if s[0] == 'run':
            doc['event'] = s[1]
            break
        if s[0] == 'step':
            doc['event'] = s[6]
            break
    return e, doc


def exit_stop(tail=30):
    """stop `tail` frames after the scene nesting returns to 0 (run/goto bookkeeping from step rows)"""
    state = {'seen_exit': None}

    def stop(frames):
        i = len(frames) - 1
        if state['seen_exit'] is None:
            for s in frames[-1]['steps']:
                if s[0] == 'exit' and outer_exited(frames):
                    state['seen_exit'] = i
        return state['seen_exit'] is not None and i >= state['seen_exit'] + tail
    return stop


def outer_exited(frames):
    """depth bookkeeping: +1 per run_scene, -1 per exit; a recording that starts inside a scene (no run in frame 0)
    starts at depth 1"""
    depth = 0 if any(s[0] == 'run' for s in frames[0]['steps']) else 1
    for f in frames:
        for s in f['steps']:
            if s[0] == 'run':
                depth += 1
            elif s[0] == 'exit':
                depth -= 1
    return depth <= 0


# ---- setups -----------------------------------------------------------------------------------------------------

REC, HIT, POS = 0x03001560, 0x0203FE00, 0x0203FDF0
ENEMY_LIST = 0x03001BB0
TYPE_TAB = 0x08711274
PLAYER = 0x03001C34


def enemies(e):
    n = e.r32(ENEMY_LIST)
    return [e.r32(ENEMY_LIST + 4 + 4 * i) for i in range(min(n, 64))]


def etype(e, q):
    return (e.r32(q + 0x224) - TYPE_TAB) // 28


def spawn_npc(e, sprite, x, y):
    """create a cutscene actor of `sprite` through the game's own spawn path (what native 0x35 does)"""
    call(e, 0x08000158, (sprite, 0, 0, 0))   # first injected call after a load may not run
    d = call(e, 0x08000158, (sprite, 0, 0, 0))
    e.w16(POS, x)
    e.w16(POS + 2, y)
    o = call(e, 0x0801111C, (0, d, POS, 0))
    call(e, 0x0801191A, (0x03001C4C, o, 0, 0))
    return o


def lethal_hit(e, target):
    """inject a lethal hit on enemy object `target` through the normal damage path 0x0800FA2A"""
    for i in range(0, 0x20, 4):
        e.w32(HIT + i, 0)
    e.w32(HIT + 4, 0x400)
    e.w32(HIT + 8, 0x086B8098)
    e.w32(HIT + 0xC, e.r32(PLAYER))
    e.w32(HIT + 0x18, 0x7FFFFFF)
    return call(e, 0x0800FA2A, (target, HIT, 0, 0))


def keep_alive(e):
    for i in range(9):
        e.w16(REC + 0x68 * i + 0x16, 9999)


def record_injected(state, frames, keys=None, pre=None, inject=None, settle=3, stop=None):
    """the scene is started by an injected game call (e.g. a lethal hit whose on-defeat callback runs run_scene in
    the same frame): snapshot after `pre`, then frame 0 = the frame in which `inject(e)` runs (emutools.call: one
    whole frame), then natural frames with keys(f) (f = 1..). Frame 0 contains the 'run' row when run_scene is
    entered during the injected frame."""
    e = emu(state)
    e.run(settle)
    if pre:
        pre(e)
    init, rec = snapshot(e)
    rec.arm()
    e.set_keys(keys(0) if keys else 0)
    inject(e)
    rec.frames.append(rec.row(keys(0) if keys else 0))
    for f in range(1, frames):
        k = keys(f) if keys else 0
        e.set_keys(k)
        e.run(1)
        rec.frames.append(rec.row(k))
        if stop and stop(rec.frames):
            break
    rec.disarm()
    e.set_keys(0)
    doc = {'kind': 'injected', 'event': None, 'skip': 0, 'init': init, 'objs': rec.objs, 'done': None,
           'frames': rec.frames}
    annotate(doc)
    for f in doc['frames']:
        hit = next((s for s in f['steps'] if s[0] in ('run', 'step')), None)
        if hit:
            doc['event'] = hit[1] if hit[0] == 'run' else hit[6]
            break
    return e, doc
