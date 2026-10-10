"""play3.py [--from NAME] [--only NAME] [--cap N] [--shots N]: play run 3 (route3.SEGMENTS) on the real game from
the soft reset (headless, muted), one continuous emulator session (the cartridge save lives in EEPROM): the segments
before --from replay their stored keys (work/run3/keys.json), the rest are driven (shops3.ShopDriver: run3_driver.Run3Driver + the shop goals) and stored.
work/run3/seg_<name>.ss at each segment start, work/run3/end.ss (+ end.eeprom) at the end."""
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 'tools/run2', 'tools/run3'):
    sys.path.insert(0, str(ROOT / d))
import sbemu  # noqa: E402  (the savedata-capable emulator lib; same core)
from route import start_emu, status  # noqa: E402
from shops3 import ShopDriver  # noqa: E402  (Run3Driver + the shop goals)
from route3 import SEGMENTS  # noqa: E402

OUT = ROOT / 'work' / 'run3'


def main():
    a = sys.argv[1:]
    frm = a[a.index('--from') + 1] if '--from' in a else None
    only = a[a.index('--only') + 1] if '--only' in a else None
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 400000
    shots = int(a[a.index('--shots') + 1]) if '--shots' in a else 0
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
        d = ShopDriver(goals)
        log = []
        prev = None
        try:
            while not d.done():
                if len(log) >= cap:
                    raise RuntimeError(f'cap at goal {d.k} {d.goals[d.k]}')
                k = d.keys(e) or 0
                e.set_keys(k)
                e.run(1)
                log.append(k)
                s = status(e)
                if (s['area'][:2], d.k) != prev:
                    prev = (s['area'][:2], d.k)
                    print(' ', len(log), d.k, d.goals[d.k] if d.k < len(d.goals) else '', s, flush=True)
                if shots and len(log) % shots == 0:
                    e.screenshot(f'/tmp/run3_{name}_{len(log):06d}.png', 1)
        except Exception as ex:  # noqa: BLE001 - keep what ran, show where
            e.screenshot(str(OUT / 'fail.png'), 2)
            e.save_file(str(OUT / 'fail.ss'))
            print(f'FAIL {name} after {len(log)} frames at goal {d.k} {d.goals[d.k] if d.k < len(d.goals) else ""}: '
                  f'{ex}', status(e), flush=True)
            sys.exit(1)
        keys[name] = log
        keyfile.write_text(json.dumps(keys))
        print(f'{name}: {len(log)} frames {time.time() - t0:.1f}s', status(e), flush=True)
        if only == name:
            break
    e.set_keys(0)
    e.save_file(str(OUT / 'end.ss'))
    (OUT / 'end.eeprom').write_bytes(sbemu.sd_read(e))
    print('total frames', sum(len(keys[n]) for n in names if n in keys))


if __name__ == '__main__':
    main()
