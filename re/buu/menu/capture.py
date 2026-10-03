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
        gd=b.hex())


def probe(e):
    return dict(lf=e.r32(0x03000F94), mode=e.r32(0x03000F90))


def setup_points(n):
    def f(e):
        e.w16(REC + 0x2C, n)
    return f


def setup_party(chars):
    def f(e):
        for c in chars:
            e.w8(REC + 0x68 * c, e.r8(REC + 0x68 * c) | 1)
    return f


def chain(*fs):
    def f(e):
        for g in fs:
            g(e)
    return f


# name: (state, setup, pre keys, keys from the START frame on)
SCENARIOS = {
    # open the pause menu, look at Status, close with B
    'status_open_close': ('buu_snakeway_start.ss', None, ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    # R / L page switches from Status, back, START closes
    'status_pages': ('buu_snakeway_start.ss', None, ['-:2'], ['START:1', '-:40', 'R:1', '-:30', 'L:1', '-:30', 'L:1', '-:30', 'R:1', '-:30', 'START:1', '-:60']),
    # stat points: the blinking LevelUp!, Use Points (A), move points, accept / cancel
    'status_alloc': ('buu_snakeway_start.ss', setup_points(5), ['-:2'],
                     ['START:1', '-:36', 'A:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'DOWN:2', '-:12', 'RIGHT:2', '-:12',
                      'LEFT:2', '-:12', 'UP:2', '-:12', 'A:2', '-:30', 'A:2', '-:12', 'RIGHT:2', '-:12', 'B:2', '-:30', 'B:2', '-:60']),
    # party tabs: Goku + Gohan + Goten + Trunks + Vegeta in the party, LEFT / RIGHT through them
    'status_tabs': ('buu_snakeway_start.ss', setup_party([1, 2, 3, 4]), ['-:2'],
                    ['START:1', '-:40', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12',
                     'LEFT:2', '-:12', 'B:2', '-:60']),
}


def run(name):
    state, setup, pre, keys = SCENARIOS[name]
    c = Cap(state)
    if setup:
        setup(c.e)
    for k in program(pre):
        c.step(k, keep=False)
    st = story(c.e)
    c.frames = []
    c.screens = []
    for k in program(keys):
        c.step(k, probe)
    d = OUT / name
    d.mkdir(parents=True, exist_ok=True)
    from PIL import Image
    for i, s in enumerate(c.screens):
        Image.fromarray(s).save(d / f'f{i:04d}.png')
    (d / 'story.json').write_text(json.dumps(st))
    (d / 'frames.json').write_text(json.dumps(dict(state=state, frames=c.frames)))
    print(name, len(c.frames), 'frames ->', d)


if __name__ == '__main__':
    for n in (sys.argv[1:] or list(SCENARIOS)):
        run(n)
