"""Emulator checks of actor / cutscene natives (0x31..0x57) in buu_snakeway_start.ss.
Spawns NPC sprite 41 next to the player and applies commands, logging positions / facing / frame counts."""
import sys
from script_nat_a_lib import asm
from script_nat_a_emu import *

e = emu('buu_snakeway_start.ss')
e.run(2)
base = e.save()
P = e.r32(PLAYER_PTR)
px, py = pos(e, P)
print('player', hex(P), (px, py), 'facing', e.r8(P + 0x10), 'world n', e.r32(WORLD + 4))
X, Y = int(px) + 24, int(py)
SPR = int(sys.argv[1]) if len(sys.argv) > 1 else 41
only = sys.argv[2].split(',') if len(sys.argv) > 2 else None


def find_new(before):
    return [o for o in world_objects(e) if o not in before]


def test(name, items, frames=400, track=None, every=0):
    if only and name not in only:
        return
    e.load(base)
    before = set(world_objects(e))
    log = []

    def cb(e, f):
        objs = find_new(before)
        if every and f % every == 0:
            log.append((f, [(hex(o), pos(e, o), e.r8(o + 0x10)) for o in objs[:2]], pos(e, P), e.r8(P + 0x10)))
    n, r = run_blocking(e, asm(items), frames, cb)
    objs = find_new(before)
    print(f'--- {name}: frames={n} new={[hex(o) for o in objs]}')
    for o in objs[:3]:
        print('    obj', hex(o), 'vt', hex(e.r32(o)), 'pos', pos(e, o), 'face', e.r8(o + 0x10), 'b8', e.r8(o + 8))
    print('    player', pos(e, P), 'face', e.r8(P + 0x10), 'b8', e.r8(P + 8), 'b11', e.r8(P + 0x11))
    for l in log[:12]:
        print('   ', l)
    return objs


S = SPR
test('spawn', [S, X, Y, ('n', 0x35), S, ('n', 0x37)], every=0)
test('spawn+face', [S, X, Y, ('n', 0x35), S, 3, ('n', 0x53)])
test('walk_to', [S, X, Y, ('n', 0x35), S, X + 40, Y + 20, 100, ('n', 0x44)], every=10)
test('move2', [S, X, Y, ('n', 0x35), S, X + 40, Y + 20, 100, ('n', 0x50)], every=10)
test('glide', [S, X, Y, ('n', 0x35), S, X + 40, Y + 20, 100, ('n', 0x49)], every=10)
test('jump', [S, X, Y, ('n', 0x35), S, X + 40, Y + 20, 30, ('n', 0x45)], every=5)
test('wait30', [S, X, Y, ('n', 0x35), S, 30, ('n', 0x4a)])
test('remove', [S, X, Y, ('n', 0x35), S, ('n', 0x4d), S, ('n', 0x37)])
test('parallel', [('n', 0x38), S, X, Y, ('n', 0x35), S, X + 40, Y, 100, ('n', 0x44), 0, int(px), int(py) + 40, 100,
                  ('n', 0x44), ('n', 0x3a)], every=10)
test('player_walk', [0, int(px) + 30, int(py), 100, ('n', 0x44)], every=10)
test('face_actor', [S, X, Y, ('n', 0x35), 0, S, ('n', 0x47), S, 0, ('n', 0x47)])
test('action3c', [0, 24, ('n', 0x3c)], every=10)
test('action3d', [0, 24, ('n', 0x3d)], every=10)
test('msg51', [0, 17, 0, ('n', 0x51)], every=10)
test('shake', [0, 4, 30, ('n', 0x57)], every=5)
test('transform', [S, X, Y, ('n', 0x35), S, 42, ('n', 0x4e), 42, ('n', 0x37), S, ('n', 0x37)])
