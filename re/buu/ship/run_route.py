"""run_route.py START.ss OUTDIR [--real | --fat] [--from MARK] [--to MARK] [--log]  -- play ship_route.goals from a state (headless,
muted), saving OUTDIR/<mark>.ss at each ('mark', name) goal, OUTDIR/end.ss at the end; OUTDIR/start.ss = START
(the goal list is built from it). --log writes OUTDIR/keys.json (per-frame keys from the start / the mark)."""
import json
import os
import shutil
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
    if frm is None:
        shutil.copyfile(start, out / 'start.ss')
    real = '--real' in a
    pick = ship_route.goals_fatbuu if '--fat' in a else ship_route.goals_real if real else ship_route.goals
    gl = pick(start_emu(str(out / 'start.ss')))
    i0 = 0
    if frm is not None:
        i0 = gl.index(('mark', frm))
        e = start_emu(str(out / f'{frm}.ss'))
    else:
        e = start_emu(start)
    gl = gl[i0:]
    d = ShipDriver(gl)
    st = {'f': 0, 'last': None, 'seg': 0}

    class Stop(Exception):
        pass

    def on_mark(name, ee):
        if name == to:
            raise Stop()
        if st['last']:
            print(f"{st['last']}: {st['f'] - st['seg']} frames -> {status(ee)}", flush=True)
        st['last'], st['seg'] = name, st['f']
        ee.save_file(str(out / f'{name}.ss'))
    d.on_mark = on_mark
    keys = []
    ring = []
    try:
        while not d.done():
            if st['f'] % 600 == 0:
                ring = (ring + [e.save()])[-3:]
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            keys.append(k)
            st['f'] += 1
            if st['f'] - st['seg'] > 200000 or status(e)['area'][:2] == [0, 2]:
                raise RuntimeError(f'stuck / game over at goal {d.k} {d.goals[d.k]} {status(e)}')
    except Stop:
        pass
    except RuntimeError:
        # the states 1200 .. 1800 frames before the failure, for debugging
        for i, b in enumerate(ring):
            e.load(b)
            e.save_file(str(out / f'fail{i}.ss'))
        if '--log' in a:
            (out / 'keys.json').write_text(json.dumps({'from': frm, 'keys': keys, 'failed': True}))
        raise
    f, last, seg_f = st['f'], st['last'], st['seg']
    print(f'{last}: {f - seg_f} frames -> {status(e)}')
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    e.screenshot(str(out / 'end.png'), scale=2)
    if '--log' in a:
        (out / 'keys.json').write_text(json.dumps({'from': frm, 'keys': keys}))
    print('total', f)


if __name__ == '__main__':
    main()
