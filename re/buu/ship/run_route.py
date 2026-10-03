"""run_route.py START.ss OUTDIR [--from LABEL] [--to LABEL] -- play ship_route segments from a state, saving
OUTDIR/<label>.ss at each segment start (and OUTDIR/end.ss); prints progress. Headless, muted."""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status  # noqa: E402
from route import start_emu  # noqa: E402
import ship_route  # noqa: E402


def main():
    a = sys.argv[1:]
    start, out = os.path.abspath(a[0]), Path(os.path.abspath(a[1]))
    frm = a[a.index('--from') + 1] if '--from' in a else None
    to = a[a.index('--to') + 1] if '--to' in a else None
    out.mkdir(parents=True, exist_ok=True)
    e = start_emu(start)
    segs = ship_route.segments(e) if frm is None else None
    if frm is not None:
        # the points split is computed from the chapter start state
        segs = ship_route.segments(start_emu(str(out / 'start.ss')) if (out / 'start.ss').exists() else e)
        segs = segs[[s[0] for s in segs].index(frm):]
        e = start_emu(str(out / f'{frm}.ss'))
    else:
        e.save_file(str(out / 'start.ss'))
    f = 0
    for label, goals in segs:
        if label == to:
            break
        e.save_file(str(out / f'{label}.ss'))
        d = ShipDriver(goals)
        n = 0
        prev = None
        while not d.done():
            e.set_keys(d.keys(e))
            e.run(1)
            f += 1
            n += 1
            s = status(e)
            key = (tuple(s['area']), s['scene'] > 0, d.k)
            if key != prev:
                prev = key
            if n > 40000:
                raise RuntimeError(f'segment {label} stuck at goal {d.k} {d.goals[d.k]} {s}')
        s = status(e)
        print(f'{label}: {n} frames -> {s}', flush=True)
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    e.screenshot(str(out / 'end.png'), scale=2)
    print('total', f)


if __name__ == '__main__':
    main()
