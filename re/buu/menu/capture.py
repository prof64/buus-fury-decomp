"""capture.py -- record menu scenarios from the emulator (headless, muted) as ground truth for
engine/crates/wf-app/tests/menu_parity.rs.

    .venv/bin/python re/buu/menu/capture.py [scenario ...]     (default: all in SCENARIOS)

Per scenario: load the save state, apply the RAM setup, run `pre` frames of keys, then from the START frame record
every video frame: keys, sound calls, the screen (work/buu/menu/<name>/fNNNN.png, git-ignored: ROM-derived) and a
sha1 of it, the mode-loop counter (0x03000F94) and the current mode (0x03000F90). `story.json` = the GameData
block decoded at the START frame (what the remake's World.story is set to), `frames.json` = the per-frame rows.
"""
import sys, json, hashlib
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from cap import Cap, program, OUT
import numpy as np

GD = 0x03001068
REC = GD + 0x4F8
DATA = Path(__file__).resolve().parents[3] / 'data' / 'menu'


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def record(e, i):
    a = REC + 0x68 * i
    n = e.r8(a + 0xC)
    return dict(
        flags=e.r8(a), fusion=e.r8(a + 2), ss_cooldown=e.r16(a + 4), form=e.r8(a + 0x14), sel=e.r8(a + 0x15),
        hp=s16(e.r16(a + 0x16)), max_hp=s16(e.r16(a + 0x18)), ep=s16(e.r16(a + 0x1A)), max_ep=s16(e.r16(a + 0x1C)),
        ss=s16(e.r16(a + 0x1E)), ss3=s16(e.r16(a + 0x20)), xp=e.r32(a + 0x24),
        str=e.r8(a + 0x28), pow=e.r8(a + 0x29), end=e.r8(a + 0x2A), spd=e.r8(a + 0x2B), points=e.r16(a + 0x2C),
        skills=[e.r8(a + 0xD + k) for k in range(n)], skill_levels=[e.r8(a + 0x2E + k) for k in range(n)],
        skill_cur=[e.r8(a + 0x2E + 2 * k) for k in range(n)], skill_max=[e.r8(a + 0x2F + 2 * k) for k in range(n)],
        equip=[e.r32(a + 0x3C + 4 * k) for k in range(4)],
        eff=[e.r16(a + 0x4C + 2 * k) for k in range(5)],
        raw=e.read(a, 0x68).hex())


def story(e):
    b = e.read(GD, 0x8A8)
    n = int.from_bytes(b[0xE4:0xE8], 'little')
    return dict(
        current=b[0], game_bits=b[1], supplies=list(b[2:2 + 83]), flags=b[0x55:0x55 + 0x8F].hex(),
        equipment=[int.from_bytes(b[0xE8 + 4 * i:0xEC + 4 * i], 'little') for i in range(n)],
        zenie=int.from_bytes(b[0x4F4:0x4F8], 'little'),
        play_base=int.from_bytes(b[0x4D0:0x4D4], 'little', signed=True), counter=e.r32(0x03000F94),
        holy=[int.from_bytes(b[o:o + 4], 'little') for o in range(0x4DC, 0x4F0, 4)],
        chars=[record(e, i) for i in range(9)],
        gd=b.hex(), **player_pos(e))


def player_pos(e):
    """the field: area (0x0300197C world / area / variant) and the player object's position (+0x1A0 / +0x1A4, 24.8 ->
    px) and facing (+0x11), for harnesses that start the remake's field at the same place"""
    p = e.r32(0x03001C34)
    out = dict(area=[e.r8(0x0300197C), e.r8(0x0300197D), e.r8(0x0300197E)])
    if 0x02000000 <= p < 0x02040000:
        out['player'] = [e.r32(p + 0x1A0) >> 8, e.r32(p + 0x1A4) >> 8, e.r8(p + 0x11)]
        # newer captures (the map screen's Scouter shows the live field): the player's direction (+0x10), the
        # other actors of the object list (0x03001C4C) [def id, x, y, direction] (def id = the actor def table
        # 0x086B6BB4 index), the camera (0x030019A4 +0x28 / +0x2C) and the RNG state (0x03004A5C)
        out['facing'] = e.r8(p + 0x10)
        ids = {}
        for i in range(10, 0x200):
            ids.setdefault(e.r32(0x086B6BB4 + 4 * i), i)
        objs = []
        for i in range(e.r32(0x03001C4C + 4)):
            o = e.r32(0x03001C4C + 8 + 4 * i)
            d = e.r32(o + 0x58)
            if o != p and d in ids:
                objs.append([ids[d], e.r32(o + 0x1A0) >> 8, e.r32(o + 0x1A4) >> 8, e.r8(o + 0x10)])
        out['objs'] = objs
        out['cam'] = [e.r32(0x030019A4 + 0x28), e.r32(0x030019A4 + 0x2C)]
    out['rng'] = [e.r32(0x03004A5C), e.r32(0x03004A60)]
    return out


def probe(e):
    return dict(lf=e.r32(0x03000F94), mode=e.r32(0x03000F90))


# scenarios: every scenarios_<area>.py next to this file (menuhelp.py has the RAM setup helpers)
SCENARIOS = {}
for _f in sorted(Path(__file__).resolve().parent.glob('scenarios_*.py')):
    _m = __import__(_f.stem)
    SCENARIOS.update(_m.SCENARIOS)


def run(name):
    """a scenario = (state, setup, pre, keys[, extra]); extra: {"open": {...} (JSON: the remake opens this screen at
    frame 0 via wf_core::menu::open_from_json instead of a key), "trigger": fn(e) (emulator side, called right
    before frame 0, e.g. an injected script call)}"""
    state, setup, pre, keys, *rest = SCENARIOS[name]
    extra = rest[0] if rest else {}
    c = Cap(state)
    if setup:
        setup(c.e)
    for k in program(pre):
        c.step(k, keep=False)
    st = story(c.e)
    if extra.get('trigger'):
        extra['trigger'](c.e)
    c.frames = []
    c.screens = []
    pr = probe
    if extra.get('probe'):
        pr = lambda e: dict(probe(e), **extra['probe'](e))  # noqa: E731
    for k in program(keys):
        c.step(k, pr)
    if extra.get('finish'):
        # scenario hook: patch extra (e.g. the "open" JSON with values read from the run)
        extra['finish'](c.frames, extra)
    d = OUT / name
    d.mkdir(parents=True, exist_ok=True)
    from PIL import Image
    for f in d.glob('f*.png'):
        f.unlink()
    for i, s in enumerate(c.screens):
        Image.fromarray(s).save(d / f'f{i:04d}.png')
    (d / 'story.json').write_text(json.dumps(st))
    (d / 'frames.json').write_text(json.dumps(dict(state=state, frames=c.frames)))
    meta = {k: v for k, v in extra.items() if not callable(v)}
    (d / 'meta.json').write_text(json.dumps(meta))
    # the committed trace (no pictures: screen sha1s) for engine/crates/wf-app/tests/menu_parity.rs
    DATA.mkdir(parents=True, exist_ok=True)
    rows = [dict(keys=f['keys'], lf=f['lf'], snd=f['snd'], h=f['h']) for f in c.frames]
    (DATA / f'{name}.json').write_text(json.dumps(dict(scenario=name, state=state, meta=meta, story=st, frames=rows), separators=(',', ':')))
    print(name, len(c.frames), 'frames ->', d)


if __name__ == '__main__':
    for n in (sys.argv[1:] or list(SCENARIOS)):
        run(n)
