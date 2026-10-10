"""Story playthrough recorder (headless, muted): the opening of the campaign played by the real game with a fixed,
closed-loop input program (route.py + driver.py), recorded frame by frame as ground truth for
engine/crates/wf-script-core/tests/playthrough.rs.

Start: work/states/buu_menus.ss, settle 3 frames, soft reset, title -> New Game (route.intro_keys). Frame 0 of the
recording = the video frame in which the new-game hook runs the prologue scene #4 (the first scene step), exactly
like data/script_lockstep/intro.json; `init` = capture.snapshot at that point.

Per video frame (capture.Recorder.row + field state):
  keys, actors [obj#, sprite, x, y, z, dir, raw, cel, tick], others, cam (22.10), io, hud, lf (logic frame counter
  0x03000F94), rng, snd (sound calls), steps (scene steps / runs / exits), area [world, area, variant],
  p [x, y, facing, state] (the player object; null while there is none), depth (run_event nesting 0x03002A5C),
  zenie, hp (current character's record), flags (story flag bytes GD+0x55.., hex, only when they changed)

usage: python3 tools/story_playthrough/record.py [--frames N] [--out data/story_playthrough/opening.json]"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'tools' / 'script_lockstep'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from capture import snapshot, SCENE_RUN, SCENE_STEP  # noqa: E402
from common_field import player_row  # noqa: E402
from driver import Driver  # noqa: E402
from route import start_emu, intro_keys, INTRO_FRAMES, ROUTE_GOALS, WS, GD, DEPTH  # noqa: E402


def first_step_frame():
    e = start_emu('reset')
    bs = [e.breakpoint(SCENE_STEP), e.breakpoint(SCENE_RUN)]
    for f in range(2000):
        e.set_keys(intro_keys(f))
        e.run(1)
        if any(h['kind'] == 'break' for h in e.hits()):
            for b in bs:
                e.clear(b)
            return f
    raise RuntimeError('no scene')


def record(frames_cap):
    f0 = first_step_frame()
    e = start_emu('reset')
    for f in range(f0):
        e.set_keys(intro_keys(f))
        e.run(1)
    init, rec = snapshot(e)
    rec.arm()
    drv = Driver(ROUTE_GOALS)
    rows = []
    flags_prev = None
    f = f0
    while len(rows) < frames_cap:
        if f < INTRO_FRAMES:
            k = intro_keys(f)
        else:
            if drv.done():
                break
            k = drv.keys(e)
        e.set_keys(k)
        e.run(1)
        row = rec.row(k)
        row['area'] = [e.r8(WS), e.r8(WS + 1), e.r8(WS + 2)]
        row['p'] = player_row(e)
        row['depth'] = e.r32(DEPTH)
        rc = 0x03001560 + 0x68 * e.r8(GD)
        row['zenie'] = e.r32(GD + 0x4F4)
        row['hp'] = e.r16(rc + 0x16)
        fl = e.read(GD + 0x55, 0x80).hex()
        if fl != flags_prev:
            row['flags'] = fl
            flags_prev = fl
        if f >= INTRO_FRAMES:
            row['goal'] = drv.k
        rows.append(row)
        f += 1
    rec.disarm()
    return {'kind': 'playthrough', 'event': 4, 'skip': f0, 'intro_frames': INTRO_FRAMES - f0, 'init': init,
            'objs': rec.objs, 'goals': [list(g) for g in ROUTE_GOALS], 'frames': rows}


def save(doc, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    frames = doc.pop('frames')
    head = json.dumps(doc, separators=(',', ':'))
    body = ',\n'.join(json.dumps(f, separators=(',', ':')) for f in frames)
    path.write_text(head[:-1] + ',"frames":[\n' + body + '\n]}\n')


def main():
    cap = int(sys.argv[sys.argv.index('--frames') + 1]) if '--frames' in sys.argv else 100000
    out = Path(sys.argv[sys.argv.index('--out') + 1]) if '--out' in sys.argv else ROOT / 'data' / 'story_playthrough' / 'opening.json'
    doc = record(cap)
    n = len(doc['frames'])
    last = doc['frames'][-1]
    save(doc, out)
    print(out, n, 'frames; ends in area', last['area'], 'p', last['p'], 'lf', last['lf'])


if __name__ == '__main__':
    main()
