"""Emulator reference frames for the frontend renderer (headless, muted: the emulator library opens no window and
no audio device). For each scenario: load a save state, play scripted keys, then dump the 240x160 screen + the
state the renderer needs (player position / facing / animation step, camera, BLDCNT/BLDALPHA, OAM).

  .venv/bin/python engine/crates/wf-render/re/capture.py   ->  work/frontend/<name>.png + <name>.json

Used by `cargo test -p wf-app --test emulator_parity` (skipped when work/frontend/ is absent). Screenshots are
ROM-derived: they stay in the git-ignored work/ directory.
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, KEY  # noqa: E402
from PIL import Image  # noqa: E402

OUT = ROOT / 'work' / 'frontend'
OUT.mkdir(parents=True, exist_ok=True)
PLAYER = 0x03001C34

K = {'R': KEY.RIGHT, 'L': KEY.LEFT, 'U': KEY.UP, 'D': KEY.DOWN, 'A': KEY.A, 'B': KEY.B, 'S': KEY.START, 'E': KEY.SELECT, '_': 0}

# name: (state, key script "R30,U12,_20")
SCENES = {
    'forest_idle': ('buu_swforest3.ss', '_30'),
    'forest_walk': ('buu_swforest3.ss', 'R40,_2'),
    'forest_under_tree_a': ('buu_swforest3.ss', 'U60,_4'),
    'forest_under_tree_b': ('buu_swforest3.ss', 'R25,U70,_4'),
    'forest_under_tree_c': ('buu_swforest3.ss', 'L40,D30,_4'),
    'forest_left': ('buu_swforest3.ss', 'L60,_3'),
    'forest_left_up': ('buu_swforest3.ss', 'L45,U25,_3'),
    'forest_up_left': ('buu_swforest3.ss', 'U30,L50,_3'),
    'forest_up_right': ('buu_swforest3.ss', 'U45,R70,_3'),
    'forest_down_left': ('buu_swforest3.ss', 'D40,L60,_3'),
    'dialog_top': (str(ROOT / 're/buu/ui/out/menus.ss'), '_1'),
    'hud_snake': (str(ROOT / 're/buu/ui/out/snake.ss'), '_1'),
    'fight_idle': ('buu_fight_swforest.ss', '_20'),
    'snakeway': ('buu_snakeway_start.ss', '_30'),
    'snakeway_walk': ('buu_snakeway_start.ss', 'U50,R20,_3'),
}


def anim_raw(e, p):
    d = e.r32(p + 0x58)
    a = e.r32(p + 0x50)
    n = e.r8(d)
    for r in range(n + 2):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def oam(e):
    o = e.oam()
    out = []
    for i in range(128):
        a0, a1, a2 = struct.unpack_from('<HHH', o, i * 8)
        if (a0 >> 8) & 3 == 2:
            continue
        out.append({'i': i, 'x': a1 & 0x1FF, 'y': a0 & 0xFF, 'shape': a0 >> 14, 'size': a1 >> 14, 'tile': a2 & 0x3FF,
                    'prio': (a2 >> 10) & 3, 'mode': (a0 >> 10) & 3, 'hflip': (a1 >> 12) & 1, 'bpp8': (a0 >> 13) & 1})
    return out


def run(name, state, script):
    e = emu(state)
    for item in script.split(','):
        keys = 0
        k = item.rstrip('0123456789')
        for c in k:
            keys |= K[c]
        e.hold(keys, int(item[len(k):] or 1))
    p = e.r32(PLAYER)
    cam = e.r32(0x080707E8)
    info = {
        'state': state, 'script': script,
        'player': {'x': e.r32(p + 0x1A0) >> 8, 'y': e.r32(p + 0x1A4) >> 8, 'dir': e.r8(p + 0x10), 'raw': anim_raw(e, p),
                   'step': e.r8(p + 0x11), 'tick': e.r16(p + 0x12)},
        'camera': [e.r32(cam) >> 10, e.r32(cam + 4) >> 10],
        'camera_raw': [e.r32(cam), e.r32(cam + 4)],
        'bldcnt': e.r16(0x04000050), 'bldalpha': e.r16(0x04000052), 'bldy': e.r16(0x04000054),
        'dispcnt': e.r16(0x04000000),
        'bgcnt': [e.r16(0x04000008 + 2 * i) for i in range(4)],
        'bgofs': [[e.r16(0x04000010 + 4 * i), e.r16(0x04000012 + 4 * i)] for i in range(4)],
        'oam': oam(e),
    }
    info['dialog_surface'] = 'dialog.bin'
    (OUT / f'{name}.dialog.bin').write_bytes(e.read(0x02032410, 160 * 64))
    rec = 0x03001560 + 0x68 * e.r8(0x03001068)
    info['record'] = {k: struct.unpack('<h', e.read(rec + o, 2))[0] for k, o in
                      (('hp', 0x16), ('hp_max', 0x18), ('ep', 0x1A), ('ep_max', 0x1C), ('ss', 0x1E), ('ss3', 0x20))}
    (OUT / f'{name}.hud.bin').write_bytes(e.read(0x06016E00, 0x500))
    Image.frombytes('RGB', (240, 160), e.pixels()).save(OUT / f'{name}.png')
    (OUT / f'{name}.json').write_text(json.dumps(info, indent=1))
    print(name, info['player'], info['camera'], hex(info['bldcnt']), hex(info['bldalpha']))


if __name__ == '__main__':
    for n, (st, sc) in SCENES.items():
        if len(sys.argv) > 1 and n not in sys.argv[1:]:
            continue
        run(n, st, sc)
