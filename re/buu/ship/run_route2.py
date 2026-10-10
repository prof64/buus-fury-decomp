"""run_route2.py START.ss OUTDIR [--from MARK] [--to MARK] [--log] -- play ship_route2.goals_run2 (run 2's ship
chapter) from a state (headless, muted), saving OUTDIR/<mark>.ss at each ('mark', name) goal, OUTDIR/end.ss at the end;
OUTDIR/start.ss = START (the goal list is built from it). --log writes OUTDIR/keys.json (per-frame keys from the start /
the mark). Marks between a Save and its Continue (r01b_saved .. r01c_continued, c07b_saved .. c07c_continued) are not
resume points: a save state does not carry the cartridge's flash (see ship_route2's note)."""
import json
import os
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_route2 import ShipDriver2, goals_run2, status  # noqa: E402
from route import start_emu  # noqa: E402


def main():
    a = sys.argv[1:]
    start, out = os.path.abspath(a[0]), Path(os.path.abspath(a[1]))
    frm = a[a.index('--from') + 1] if '--from' in a else None
    to = a[a.index('--to') + 1] if '--to' in a else None
    out.mkdir(parents=True, exist_ok=True)
    if frm is None:
        shutil.copyfile(start, out / 'start.ss')
    gl = goals_run2(start_emu(str(out / 'start.ss')))
    i0 = 0
    if frm is not None:
        i0 = gl.index(('mark', frm))
        e = start_emu(str(out / f'{frm}.ss'))
    else:
        e = start_emu(start)
    gl = gl[i0:]
    d = ShipDriver2(gl)
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
    over = 0
    try:
        while not d.done():
            if st['f'] % 600 == 0:
                ring = (ring + [e.save()])[-3:]
            gk = d.k
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            keys.append(k)
            st['f'] += 1
            if '--trace' in a and d.k != gk:
                print('  goal', d.k, str(d.goals[d.k] if not d.done() else None)[:80], 'f', st['f'], status(e), flush=True)
            # (the planned death: the game over screen and the title are area 0-2 for a while; an unplanned one
            # stays there)
            over = over + 1 if status(e)['area'][:2] == [0, 2] else 0
            if st['f'] - st['seg'] > 250000 or over > 6000:
                raise RuntimeError(f'stuck / game over at goal {d.k} {d.goals[d.k]} {status(e)}')
    except Stop:
        pass
    except Exception:
        for i, b in enumerate(ring):
            e.load(b)
            e.save_file(str(out / f'fail{i}.ss'))
        print('failed at goal', d.k, d.goals[d.k] if not d.done() else None, status(e), 'frame', st['f'], flush=True)
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
