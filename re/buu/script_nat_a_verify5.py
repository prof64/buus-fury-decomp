"""Screenshots of the attack-style cutscene commands: 0x3b, 0x3e, 0x41, 0x42, 0x46 (player = actor 0)."""
import sys
from PIL import Image
from script_nat_a_lib import asm
from script_nat_a_emu import *

e = emu('buu_snakeway_start.ss')
e.run(2)
base = e.save()
P = e.r32(PLAYER_PTR)
px, py = map(int, pos(e, P))
only = sys.argv[1].split(',') if len(sys.argv) > 1 else None


def test(name, items, frames=300, snaps=(2, 8, 14, 20, 30, 45)):
    if only and name not in only:
        return
    e.load(base)
    ims = []

    def cb(e, f):
        if f in snaps:
            ims.append(Image.frombytes('RGB', (240, 160), e.pixels()))
    n, r = run_blocking(e, asm(items), frames, cb)
    W = Image.new('RGB', (240 * max(1, len(ims)), 160))
    for i, im in enumerate(ims):
        W.paste(im, (240 * i, 0))
    W.save(WORK / 'script' / f'nat_a_{name}.png')
    print(f'--- {name}: frames={n} face={e.r8(P + 0x10)}')


test('blast3b', [0, 3, 40, ('n', 0x3b)])
test('blast3e', [0, px + 80, py, ('n', 0x3e)])
test('blast42', [0, px + 80, py, 300, ('n', 0x42)])
test('spawn_blast41', [41, px + 60, py, ('n', 0x35), 0, 41, 300, ('n', 0x41)])
test('beam46', [0, 3, 60, ('n', 0x46)], snaps=(5, 20, 30, 40, 60, 80))
