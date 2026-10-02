"""Emulator checks with screenshots: HUD hide/show (0x30/0x2f), effect spawns (0x3f/0x40/0x48), camera pans
(0x31/0x32), camera follow (0x33/0x34), map object spawn (0x36), shake (0x57), tint (0x52), aura (0x55).
Writes work/buu/script/nat_a_<test>.png grids."""
import sys
from PIL import Image
from script_nat_a_lib import asm
from script_nat_a_emu import *

e = emu('buu_snakeway_start.ss')
e.run(2)
base = e.save()
P = e.r32(PLAYER_PTR)
px, py = map(int, pos(e, P))
CAM = 0x030019BC
only = sys.argv[1].split(',') if len(sys.argv) > 1 else None


def cam():
    return [e.r32(CAM + 4 * k) for k in range(4)]


def test(name, items, frames=300, snaps=(0, 10, 30, 60)):
    if only and name not in only:
        return
    e.load(base)
    ims = []
    camlog = []

    def cb(e, f):
        if f in snaps:
            ims.append(Image.frombytes('RGB', (240, 160), e.pixels()))
        if f % 10 == 0:
            camlog.append((f, [hex(x) for x in cam()[:2]]))
    n, r = run_blocking(e, asm(items), frames, cb)
    for k in range(2):
        e.run(10)
        ims.append(Image.frombytes('RGB', (240, 160), e.pixels()))
    W = Image.new('RGB', (240 * len(ims), 160))
    for i, im in enumerate(ims):
        W.paste(im, (240 * i, 0))
    W.save(WORK / 'script' / f'nat_a_{name}.png')
    print(f'--- {name}: frames={n} cam={camlog[:8]}')


test('hud_hide', [('n', 0x30)])
test('hud_show', [('n', 0x30), 0, 30, ('n', 0x4a), ('n', 0x2f)])
test('fx3f', [0, ('n', 0x3f), 0, 60, ('n', 0x4a)], snaps=(0, 5, 10, 20, 40))
test('fx40', [0, 16, 16, ('n', 0x40), 0, 60, ('n', 0x4a)], snaps=(0, 5, 10, 20, 40))
test('fx48', [0, px + 20, py, ('n', 0x48), 0, 60, ('n', 0x4a)], snaps=(0, 5, 10, 20, 40))
test('cam32', [px - 120 + 100, py - 80, 2, ('n', 0x32)], snaps=(0, 20, 40, 60))
test('cam31', [0, 2, ('n', 0x31)], snaps=(0, 20, 40))
test('cam33', [0, ('n', 0x33), 0, px + 60, py, 100, ('n', 0x44), ('n', 0x34)], snaps=(0, 20, 40, 60))
test('shake', [0, 8, 30, ('n', 0x57)], snaps=(0, 3, 6, 9))
test('tint', [0, 255, 255, 255, 255, ('n', 0x52), 0, 30, ('n', 0x4a)], snaps=(0, 5, 20))
test('tint_red', [0, 255, 0, 0, 255, ('n', 0x52), 0, 30, ('n', 0x4a)], snaps=(0, 5, 20))
test('aura', [0, 1, 0, 0, 0, ('n', 0x55), 0, 30, ('n', 0x4a)], snaps=(0, 5, 20))
test('mapobj0', [0, ('n', 0x36)], snaps=(0, 5))
test('flash4c', [0, 255, 0, 0, 30, ('n', 0x4c)], snaps=(0, 5, 10, 15, 20, 25))
test('msg43', [0, ('n', 0x43), 0, 30, ('n', 0x4a)], snaps=(0, 5, 20))
test('anim3c', [0, 24, ('n', 0x3c)], snaps=(0, 5, 20, 40, 80))
test('pose51', [0, 17, 0, ('n', 0x51), 0, 30, ('n', 0x4a)], snaps=(0, 5, 20))
