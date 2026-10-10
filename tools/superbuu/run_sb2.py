"""run_sb2.py START.ss OUTDIR [--sav FILE] [--from MARK] [--to MARK] [--log] [--trace] -- play sbroute2.goals_run2 (run 2's
Super Buu + Kid Buu part) from a state (headless, muted), saving OUTDIR/<mark>.ss at each ('mark', name) goal and
OUTDIR/end.ss (+ end.png) at the end; OUTDIR/start.ss = START (the goal list is built from it). --log writes
OUTDIR/keys.json (per-frame keys from START / the --from mark). The emulator is the savedata-capable one
(tools/superbuu/sbemu.py, same core): the in-game saves live in its EEPROM for the whole session. Marks after the
first Save are not resume points for a final run (a .ss does not carry the EEPROM)."""
import json
import os
import shutil
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import sbemu  # noqa: E402,F401  (the savedata-capable emulator lib first)
from sbroute2 import SbDriver2, goals_run2, status  # noqa: E402
from route import start_emu as _start_emu  # noqa: E402

SAV = None


def start_emu(path):
    # --sav FILE: the cartridge save memory (EEPROM: saves + options) loaded before the first frame
    e = _start_emu(path)
    if SAV:
        sbemu.sd_write(e, open(SAV, 'rb').read())
    return e


def main():
    global SAV
    a = sys.argv[1:]
    SAV = os.path.abspath(a[a.index('--sav') + 1]) if '--sav' in a else None
    start, out = os.path.abspath(a[0]), Path(os.path.abspath(a[1]))
    frm = a[a.index('--from') + 1] if '--from' in a else None
    to = a[a.index('--to') + 1] if '--to' in a else None
    out.mkdir(parents=True, exist_ok=True)
    if frm is None and Path(start) != out / 'start.ss':
        shutil.copyfile(start, out / 'start.ss')
    gl = goals_run2(start_emu(str(out / 'start.ss')))
    i0 = 0
    if frm is not None:
        i0 = gl.index(('mark', frm))
        e = start_emu(str(out / f'{frm}.ss'))
    else:
        e = start_emu(str(out / 'start.ss'))
    d = SbDriver2(gl[i0:])
    st = {'f': 0, 'last': frm, 'seg': 0, 't0': time.time()}

    class Stop(Exception):
        pass

    def on_mark(name, ee):
        if st['last']:
            print(f"{st['last']}: {st['f'] - st['seg']} frames -> {status(ee)} ({time.time() - st['t0']:.0f}s)", flush=True)
        st['last'], st['seg'] = name, st['f']
        ee.save_file(str(out / f'{name}.ss'))
        if name == to:
            raise Stop()
    d.on_mark = on_mark
    keys = []
    ring = []
    over = 0
    try:
        while not d.done():
            if st['f'] % 600 == 0:
                ring = (ring + [e.save()])[-3:]
            gk = d.k
            k = d.keys(e) or 0
            e.set_keys(k)
            e.run(1)
            keys.append(k)
            st['f'] += 1
            if os.environ.get('SB2_KEYS') and int(os.environ['SB2_KEYS'].split('..')[0]) <= st['f'] <= int(os.environ['SB2_KEYS'].split('..')[1]):
                print('  f', st['f'], 'goal', d.k, str(d.goals[d.k])[:40], 'key', k, 'prog', len(d.prog), status(e), flush=True)
            if '--trace' in a and d.k != gk:
                print('  goal', d.k, str(d.goals[d.k] if not d.done() else None)[:90], 'f', st['f'], status(e), flush=True)
            _s = status(e)
            over = over + 1 if _s['area'][:2] == [0, 2] and _s['hp'] == 0 else 0
            if st['f'] - st['seg'] > 300000 or over > 8000:
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
    print(f"{st['last']}: {st['f'] - st['seg']} frames -> {status(e)}")
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    e.screenshot(str(out / 'end.png'), scale=2)
    if '--log' in a:
        (out / 'keys.json').write_text(json.dumps({'from': frm, 'keys': keys}))
    print('total', st['f'])


if __name__ == '__main__':
    main()
