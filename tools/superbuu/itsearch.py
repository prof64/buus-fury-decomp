"""itsearch.py: Super Buu 0x77 (20-7): find the Instant Transmission timing that advances the phase.
From the fight start, run until Buu starts a glide (position changes); for each delay d, release-time search:
wait d frames, hold B (IT pick: the world freezes), steer the cursor onto Buu, release, then watch for an event."""
import sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
import sbdriver  # noqa
from driver import Driver  # noqa
from route import start_emu, status  # noqa
from common_field import enemy_detail  # noqa

PRE = [('exitto', '20-6'), ('wait', 5), ('boss', 12000), ('walk', 96, 44), ('talk', 'U'), ('eventsw', 240), ('wait', 20),
       ('walk', int(sys.argv[1]), int(sys.argv[2]))]


def buu(e):
    b = [(d['x'] >> 8, d['y'] >> 8) for d in enemy_detail(e) if d.get('type') == 119]
    return b[0] if b else None


e = start_emu(str(ROOT / 'work' / 'superbuu' / 'm_fusion_saga.ss'))
d = Driver(PRE)
while not d.done():
    e.set_keys(d.keys(e))
    e.run(1)
last = buu(e)
n = 0
while True:
    e.set_keys(0)
    e.run(1)
    n += 1
    b = buu(e)
    if b != last and last is not None:
        break
    last = b
print('glide starts after', n, 'at', last, '->', b, flush=True)
base = e.save()


def aim(e):
    """hold B; drive the cursor (model of player.rs it_select) onto Buu's body centre; returns frames used"""
    s = status(e)
    x, y = s['p'][0] + 8, s['p'][1] - 8
    cx, cy, vx, vy = x << 8, y << 8, 0, 0
    bx, by = buu(e)
    tx, ty = (bx + 16) << 8, (by - 8) << 8
    k = 0
    for f in range(120):
        keys = 2
        for ax in 'xy':
            c, v, t = (cx, vx, tx) if ax == 'x' else (cy, vy, ty)
            want = 1 if t - c > 3 * v + 512 else -1 if t - c < 3 * v - 512 else 0
            keys |= (16 if want > 0 else 32 if want < 0 else 0) if ax == 'x' else (128 if want > 0 else 64 if want < 0 else 0)
            v = (v * 3) // 4 + 256 * want
            if ax == 'x':
                vx, cx = v, c + v
            else:
                vy, cy = v, c + v
        e.set_keys(keys)
        e.run(1)
        if abs(cx - tx) < 4 << 8 and abs(cy - ty) < 4 << 8:
            k += 1
            if k > 3:
                break
    e.set_keys(0)
    e.run(1)


for dly in range(int(sys.argv[3]), int(sys.argv[4])):
    e.load(base)
    for _ in range(dly):
        e.set_keys(0)
        e.run(1)
    aim(e)
    ev = None
    for f in range(160):
        e.set_keys(0)
        e.run(1)
        if status(e)['scene'] > 0:
            ev = f
            break
    print(dly, 'event' if ev is not None else '-', ev, status(e)['p'], buu(e), status(e)['hp'], flush=True)
