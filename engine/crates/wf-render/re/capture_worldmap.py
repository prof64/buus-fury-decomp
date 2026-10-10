"""World-map (area 0-50) reference frames for the mode-7 renderer (headless, muted emulator library).

  .venv/bin/python engine/crates/wf-render/re/capture_worldmap.py  ->  work/frontend_wm/<name>.png + .json

Start state: work/buu/wm_start.ss (the 8-1 capsule's arrival on the world map, tools/field/rec_worldmap.py setup).
Per scene: the 3D camera (map object 0x030019A4 +0 x, +4 y, +8 height, +0xC heading, +0x10 pitch) after the last
frame and after the frame before it, the flyer (x, y 24.8, z 22.10, raw anim, anim frame, facing, sprite def), the
label object's string id, the mini-map marker object, the screen and the OAM. Used by
`crates/wf-app/tests/emulator_parity.rs` (world-map part; skipped when the directory is absent).
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from common import emu, KEY  # noqa: E402
from PIL import Image  # noqa: E402

OUT = ROOT / 'work' / 'frontend_wm'
OUT.mkdir(parents=True, exist_ok=True)
STATE = str(ROOT / 'work' / 'buu' / 'wm_start.ss')
MAPOBJ = 0x030019A4
PLAYER = 0x03001C34
OBJW = 0x03001C4C
SHADOW_OAM = 0x03003498
K = {'R': KEY.RIGHT, 'L': KEY.LEFT, 'U': KEY.UP, 'D': KEY.DOWN, 'A': KEY.A, 'B': KEY.B, '_': 0}

SCENES = {
    'wm_start': '_2',
    'wm_forward': 'U30',
    'wm_turn_right': 'U20,UR40',
    'wm_turn_left': 'U40,UL60,U10',
    'wm_climb': 'B40,U30',
    'wm_high': 'B90,U40,UR20',
    'wm_low': 'A12,U30',
    'wm_reverse': 'U30,D40,DL20',
    'wm_far_east': 'U20,UR60,U200',
    'wm_far_north': 'U300',
    'wm_far_west': 'UL70,U250',
    'wm_spin': 'UR160',
}


def s32(v):
    return v - (1 << 32) if v & 0x80000000 else v


def cam(e):
    return [s32(e.r32(MAPOBJ + 4 * i)) for i in range(5)]


def anim_raw(e, p):
    d = e.r32(p + 0x58)
    a = e.r32(p + 0x50)
    for r in range(40):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def flyer(e):
    p = e.r32(PLAYER)
    return {'x': s32(e.r32(p + 0x1A0)), 'y': s32(e.r32(p + 0x1A4)), 'z': s32(e.r32(p + 0x1A8)), 'raw': anim_raw(e, p),
            'step': e.r8(p + 0x11), 'dir': e.r8(p + 0x10), 'def': '0x%08X' % e.r32(p + 0x58)}


def objects(e):
    n = e.r32(OBJW + 4)
    return [e.r32(OBJW + 8 + 4 * i) for i in range(min(n, 400))]


def run(name, script):
    e = emu(STATE)
    steps = []
    for item in script.split(','):
        k = item.rstrip('0123456789')
        keys = 0
        for c in k:
            keys |= K[c]
        steps += [keys] * int(item[len(k):] or 1)
    prev = prev2 = cam(e)
    fprev = flyer(e)
    hist = []
    for keys in steps:
        prev2 = prev
        prev = cam(e)
        fprev = flyer(e)
        e.set_keys(keys)
        e.run(1)
        hist = (hist + [{'cam': cam(e), 'flyer': flyer(e), 'shadow_oam': e.read(SHADOW_OAM, 1024).hex()}])[-4:]
    e.set_keys(0)
    t = e.r32(MAPOBJ + 0xA8)
    table = []
    for v in range(56, 160):
        pa, pb, pc, pd = struct.unpack('<hhhh', e.read(t + 16 * v, 8))
        x, y = struct.unpack('<ii', e.read(t + 16 * v + 8, 8))
        table.append([(pa & 0xFFFF) | ((pb & 0xFFFF) << 16), (pc & 0xFFFF) | ((pd & 0xFFFF) << 16), x, y])
    p = e.r32(PLAYER)
    info = {
        'script': script,
        'cam': cam(e),
        'cam_prev': prev,
        'cam_prev2': prev2,
        'flyer': flyer(e),
        'flyer_prev': fprev,
        'bldcnt': e.r16(0x04000050), 'bldalpha': e.r16(0x04000052), 'dispcnt': e.r16(0x04000000),
        'oam': e.oam().hex(),
        'history': hist,
        'table': table,
    }
    for o in objects(e):
        vt = e.r32(o)
        if vt == 0x0804D8C8:  # bottom label
            info['label'] = {'string': e.r32(o + 0x18), 'shown': e.r32(o + 0x14)}
        if vt == 0x0804D8A0:  # mini-map marker
            info['marker'] = [s32(e.r32(o + 0x10 + 4 * i)) for i in range(4)]
    Image.frombytes('RGB', (240, 160), e.pixels()).save(OUT / f'{name}.png')
    (OUT / f'{name}.json').write_text(json.dumps(info, indent=1))
    print(name, info['cam'], info['flyer'], info.get('label'), info.get('marker'))


if __name__ == '__main__':
    for n, sc in SCENES.items():
        if len(sys.argv) > 1 and n not in sys.argv[1:]:
            continue
        run(n, sc)
