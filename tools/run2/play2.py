"""play2.py [--from NAME] [--only NAME] [--cap N]: play run 2 (route2.SEGMENTS) on the real game from the soft reset
(headless, muted), one continuous emulator session (the in-game save lives in the cartridge's EEPROM, which save
states do not carry): the segments before --from replay their stored keys (work/run2/keys.json), the rest are
driven (run2_driver.Run2Driver) and their keys stored. work/run2/seg_<name>.ss at each segment start (exploration
only), work/run2/end.ss at the end."""
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 'tools/run2'):
    sys.path.insert(0, str(ROOT / d))
import sbemu  # noqa: E402,F401  (the savedata-capable emulator lib; same core)
from route import start_emu, status  # noqa: E402
from run2_driver import Run2Driver  # noqa: E402
from route2 import SEGMENTS  # noqa: E402

OUT = ROOT / 'work' / 'run2'


def main():
    a = sys.argv[1:]
    frm = a[a.index('--from') + 1] if '--from' in a else None
    only = a[a.index('--only') + 1] if '--only' in a else None
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 400000
    # --shots N: /tmp/run2_<seg>_<frame>.png every N driven frames (--trace: and the status)
    global shots, trace
    shots = int(a[a.index('--shots') + 1]) if '--shots' in a else 0
    trace = '--trace' in a
    OUT.mkdir(parents=True, exist_ok=True)
    keyfile = OUT / 'keys.json'
    keys = json.loads(keyfile.read_text()) if keyfile.exists() else {}
    names = [n for n, _ in SEGMENTS]
    i0 = names.index(frm) if frm else 0
    for n in names[i0:]:
        keys.pop(n, None)
    e = start_emu('reset')
    for name, goals in SEGMENTS:
        e.save_file(str(OUT / f'seg_{name}.ss'))
        if name in keys:
            for k in keys[name]:
                e.set_keys(k)
                e.run(1)
            continue
        t0 = time.time()
        if isinstance(goals, tuple) and goals[0] == 'KEYS':
            # a part another agent played from the chain's end state (segments.md §0, run 2 hand-off): its key log
            src = ROOT / goals[1]
            if src.exists():
                doc = json.loads(src.read_text())
                log = doc['keys'] if isinstance(doc, dict) else doc
            else:
                # work/ is not tracked: the packed copy (data/realrun2/NNN_<name>.keys.gz)
                import gzip
                packed = sorted((ROOT / 'data' / 'realrun2').glob(f'*_{name}.keys.gz'))
                if not packed:
                    raise SystemExit(f'{name}: no key log ({src} / data/realrun2)')
                log = [k for k, n in json.loads(gzip.open(packed[0], 'rt').read()) for _ in range(n)]
            for k in log:
                e.set_keys(k)
                e.run(1)
            keys[name] = log
            keyfile.write_text(json.dumps(keys))
            print(f'{name}: {len(log)} frames (key log {goals[1]})', status(e), flush=True)
            continue
        if goals == 'SHIP2':
            # the ship chapter: re/buu/ship/ship_route2.py's goals, built from the state here, its own driver
            sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
            from ship_route2 import ShipDriver2, goals_run2
            d = ShipDriver2(goals_run2(e))
            cap = max(cap, 1200000)
        else:
            d = Run2Driver(goals)
        log = []
        try:
            while not d.done():
                if len(log) >= cap:
                    raise RuntimeError(f'cap at goal {d.k} {d.goals[d.k]}')
                k = d.keys(e)
                e.set_keys(k)
                e.run(1)
                log.append(k)
                if shots and len(log) % shots == 0:
                    e.screenshot(f'/tmp/run2_{name}_{len(log):06d}.png', 1)
                    if trace:
                        print(' ', len(log), d.k, d.goals[d.k] if d.k < len(d.goals) else '', status(e), flush=True)
        except Exception as ex:  # noqa: BLE001 - keep what ran, show where
            e.screenshot('/tmp/run2_fail.png', 2)
            e.save_file(str(OUT / 'fail.ss'))
            print(f'FAIL {name} after {len(log)} frames at goal {d.k} {d.goals[d.k] if d.k < len(d.goals) else ""}: {ex}',
                  status(e), flush=True)
            sys.exit(1)
        keys[name] = log
        keyfile.write_text(json.dumps(keys))
        print(f'{name}: {len(log)} frames {time.time() - t0:.1f}s', status(e), flush=True)
        if only == name:
            break
    e.set_keys(0)
    e.save_file(str(OUT / 'end.ss'))
    print('total frames', sum(len(keys[n]) for n in names if n in keys))


if __name__ == '__main__':
    main()
