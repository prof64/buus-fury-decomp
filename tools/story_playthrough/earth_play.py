"""Play the Earth chapters' route (earth_route.SEGMENTS) on the real game (headless, muted) from the chapter start
state, saving the state at every segment start (work/earth/seg_<name>.ss) and at the end (work/earth/seg_end.ss),
plus the key log (work/earth/keys.json: per segment the per-frame keys).

usage: python3 tools/story_playthrough/earth_play.py [--from NAME] [--only NAME] [--cap N]
  --from NAME   start at that segment from its saved state (earlier segments must have been played)"""
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from earth_driver import EarthDriver  # noqa: E402
from earth_route import SEGMENTS  # noqa: E402
from route import start_emu, status  # noqa: E402

OUT = ROOT / 'work' / 'earth'
START = OUT / 'chapter_start.ss'


def main():
    args = sys.argv[1:]
    frm = args[args.index('--from') + 1] if '--from' in args else None
    only = args[args.index('--only') + 1] if '--only' in args else None
    cap = int(args[args.index('--cap') + 1]) if '--cap' in args else 200000
    names = [n for n, _ in SEGMENTS]
    i0 = names.index(frm) if frm else 0
    keyfile = OUT / 'keys.json'
    keys = json.loads(keyfile.read_text()) if keyfile.exists() and frm else {}
    seg0 = OUT / f'seg_{names[i0]}.ss'
    if i0 and not seg0.exists():
        seg0 = OUT / 'seg_end.ss'      # the first new segment: the previous run's end
    e = start_emu(str(START if i0 == 0 else seg0))
    for name, goals in SEGMENTS[i0:]:
        e.save_file(OUT / f'seg_{name}.ss')
        t0 = time.time()
        d = EarthDriver(goals)
        log = []
        while not d.done():
            if len(log) >= cap:
                print('CAP', name, d.k, d.goals[d.k], status(e))
                keys[name] = log
                keyfile.write_text(json.dumps(keys))
                return
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            log.append(k)
        keys[name] = log
        print(f'{name}: {len(log)} frames {time.time() - t0:.1f}s', status(e), flush=True)
        if only == name:
            break
    e.set_keys(0)
    e.save_file(OUT / 'seg_end.ss')
    keyfile.write_text(json.dumps(keys))


if __name__ == '__main__':
    main()
