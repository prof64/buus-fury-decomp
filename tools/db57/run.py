"""run.py ROUTE START.ss OUTDIR [--from MARK] [--to MARK]  -- play tools/db57/routes.py ROUTES[ROUTE] from a state
(headless, muted), saving OUTDIR/<mark>.ss at each ('mark', name) goal and OUTDIR/end.ss; OUTDIR/keys.json = the
per-frame keys from the start (or from the mark), in re/buu/ship/rec_segments.py's format."""
import json
import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from dbdriver import DbDriver, status  # noqa: E402
from route import start_emu  # noqa: E402
import importlib  # noqa: E402


def find_route(name):
    """(goal list builder, driver class) from routes.py / routes6.py / routes7.py / routesx.py: each module has a
    ROUTES dict and optionally DRIVER (a DbDriver subclass)"""
    for mod in ('routes', 'routes6', 'routes7', 'routesx'):
        try:
            m = importlib.import_module(mod)
        except ModuleNotFoundError:
            continue
        if name in m.ROUTES:
            return m.ROUTES[name], getattr(m, 'DRIVER', DbDriver)
    raise KeyError(name)


def main():
    a = sys.argv[1:]
    name, start, out = a[0], os.path.abspath(a[1]), Path(os.path.abspath(a[2]))
    frm = a[a.index('--from') + 1] if '--from' in a else None
    to = a[a.index('--to') + 1] if '--to' in a else None
    out.mkdir(parents=True, exist_ok=True)
    if frm is None:
        shutil.copyfile(start, out / 'start.ss')
    mk, drv = find_route(name)
    gl = mk()
    i0 = 0
    if frm is not None:
        i0 = gl.index(('mark', frm))
        e = start_emu(str(out / f'{frm}.ss'))
    else:
        e = start_emu(start)
    d = drv(gl[i0:])
    st = {'f': 0, 'last': None, 'seg': 0}

    class Stop(Exception):
        pass

    def on_mark(nm, ee):
        if nm == to:
            raise Stop()
        if st['last']:
            print(f"{st['last']}: {st['f'] - st['seg']} frames -> {status(ee)}", flush=True)
        st['last'], st['seg'] = nm, st['f']
        ee.save_file(str(out / f'{nm}.ss'))
    d.on_mark = on_mark
    keys = []
    ring = []
    fail = None
    try:
        while not d.done():
            if st['f'] % 600 == 0:
                ring = (ring + [e.save()])[-3:]
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            keys.append(k)
            st['f'] += 1
            ar = status(e)['area'][:2]
            if ar != st.get('ar'):
                st['ar'] = ar
                if os.environ.get('V'):
                    print(st['f'], 'area', ar, status(e), flush=True)
            if st['f'] - st['seg'] > 60000 or status(e)['area'][:2] == [0, 2] and status(e)['hp'] == 0:
                raise RuntimeError(f'stuck / game over at goal {d.k} {d.goals[d.k]} {status(e)}')
    except Stop:
        pass
    except RuntimeError as ex:
        fail = ex
        e.save_file(str(out / 'fail.ss'))
        e.screenshot(str(out / 'fail.png'), scale=2)
        for i, b in enumerate(ring):
            e.load(b)
            e.save_file(str(out / f'fail{i}.ss'))
    (out / ('keys.json' if frm is None else f'keys_{frm}.json')).write_text(
        json.dumps({'from': frm, 'keys': keys, 'failed': fail is not None}))
    if fail:
        raise fail
    print(f"{st['last']}: {st['f'] - st['seg']} frames -> {status(e)}")
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    e.screenshot(str(out / 'end.png'), scale=2)
    print('total', st['f'])


if __name__ == '__main__':
    main()
