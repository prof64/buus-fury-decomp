"""Dragon Ball dungeons 1-4: routes played by the real game from work/db/<chapter>_start.ss (mksave_db.py), with the
ship chapter's closed-loop driver (re/buu/ship/ship_driver.py: nav / fightp / keys / story ... goals).

usage: db_route.py CHAPTER [--from MARK] [--to MARK]
  -> work/db/<chapter>/{start.ss, <mark>.ss, end.ss, keys.json}  (the layout re/buu/ship/rec_segments.py cuts:
     .venv/bin/python re/buu/ship/rec_segments.py work/db/db1 work/db/db1/segs --prefix db1)"""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
from db_driver import DbDriver, status, KEY  # noqa: E402
from route import start_emu  # noqa: E402

OUT = ROOT / 'work' / 'db'
U, D, L, R, A, B, START, RS = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B, KEY.START, KEY.R


def nav(w, a, fight=True):
    return ('nav', w, a, fight)


def equip(row):
    """pause menu -> Equip page (R, R) -> slot row -> the list's first item -> Equip -> back to the field"""
    return ('keys', [(START, 3, 70), (RS, 3, 60), (RS, 3, 60)] + [(D, 3, 15)] * row +
            [(A, 3, 40), (A, 3, 40), (A, 3, 60), (B, 3, 60), (B, 3, 90)])


def db1(e):
    """Thieves Den: the Bandana on (Accessories), Trunks opens 9-1's gate (s517, level 80), 9-2 .. 9-6 (bandits),
    the two door guards of 9-10 (s349 -> s350: flag 80), the hall 9-11, the stairs 9-12 .. 9-14, the Bandit King's
    loot room 9-15 (s357; the 1-Star Dragon Ball s362: flag 112, the monkey takes the Bandana, four bandits attack),
    back out through the now hostile den to 9-1 and the world map (flag 82)."""
    return [
        ('mark', 'a_start'),
        ('wait', 300), equip(3), ('wait', 30),
        ('walk', 232, 166), ('hold', U, 12), ('press', A), ('expect', 60), ('wait', 30),  # the gate
        nav(9, 2), ('mark', 'b_9-2'),
        nav(9, 4), ('mark', 'b2_9-4'),
        # 9-5: the Juggernaut (a tank, 1552 HP) fills the canyon; its crew of four climbs out when it is destroyed
        nav(9, 5, False), ('mark', 'b3_9-5'), ('tank', 58, 6000), ('wait', 30), ('events',),
        nav(9, 6), nav(9, 10, False),
        ('mark', 'c_9-10'),
        ('walk', 136, 96), ('talk', 'U'), ('wait', 30),                                  # s349 -> s350
        nav(9, 11, False), ('mark', 'd_9-11'),
        ('walk', 267, 84), ('hold', U, 40), ('until', 9, 12),                            # the door (opens on touch)
        nav(9, 15, False), ('mark', 'e_9-15'),
        ('expect', 200), ('wait', 20),                                                   # s357
        ('walk', 105, 156), ('hold', U, 12), ('press', A), ('expect', 60),               # s362: the Dragon Ball
        ('mark', 'f_ball'),
        ('fightp', 20000, 99999, 6), ('wait', 30),
        nav(9, 11), ('mark', 'g_9-11'),
        nav(9, 6), ('mark', 'g2_9-6'),
        nav(9, 5, False), ('tank', 58, 6000), ('wait', 30), ('events',),                 # the Juggernaut again, from above
        nav(9, 1), ('mark', 'h_9-1'),
        ('walk', 40, 262), ('hold', U, 12), ('press', A), ('wait', 300),                 # the world map
        ('mark', 'i_map'),
    ]


ROUTES = {'db1': db1}


def main(routes=None, driver=None):
    """routes: {chapter: goals(e)} (default ROUTES); driver: a DbDriver subclass (the other dungeons' route files
    call main with theirs)"""
    routes = routes or ROUTES
    driver = driver or DbDriver
    a = sys.argv[1:]
    name = a[0]
    out = OUT / name
    frm = a[a.index('--from') + 1] if '--from' in a else None
    to = a[a.index('--to') + 1] if '--to' in a else None
    out.mkdir(parents=True, exist_ok=True)
    if frm is None:
        shutil.copyfile(OUT / f'{name}_start.ss', out / 'start.ss')
    gl = routes[name](start_emu(str(out / 'start.ss')))
    i0 = 0
    if frm is not None:
        i0 = gl.index(('mark', frm))
        e = start_emu(str(out / f'{frm}.ss'))
    else:
        e = start_emu(str(out / 'start.ss'))
    d = driver(gl[i0:])
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
    prev = None
    try:
        while not d.done():
            if st['f'] % 600 == 0:
                ring = (ring + [e.save()])[-3:]
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            keys.append(k)
            st['f'] += 1
            s = status(e)
            key = (tuple(s['area']), d.k)
            if key != prev and '-v' in a:
                print(st['f'], 'goal', d.k, d.goals[d.k] if not d.done() else None, s, flush=True)
            prev = key
            if st['f'] - st['seg'] > 60000 or s['area'][:2] == [0, 2]:
                raise RuntimeError(f'stuck / game over at goal {d.k} {d.goals[d.k]} {s}')
    except Stop:
        pass
    except RuntimeError as ex:
        print('FAILED', ex)
        for i, b in enumerate(ring):
            e.load(b)
            e.save_file(str(out / f'fail{i}.ss'))
        e.screenshot(str(out / 'fail.png'), scale=2)
        (out / 'keys.json').write_text(json.dumps({'from': frm, 'keys': keys, 'failed': True}))
        raise
    print(f"{st['last']}: {st['f'] - st['seg']} frames -> {status(e)}")
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    e.screenshot(str(out / 'end.png'), scale=2)
    (out / 'keys.json').write_text(json.dumps({'from': frm, 'keys': keys}))
    print('total', st['f'])


if __name__ == '__main__':
    main()
