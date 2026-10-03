"""The Super Buu saga -> Kid Buu route, played by the real game from work/superbuu/sb_start.ss (mksave.py superbuu).

Goals: tools/story_playthrough/driver.py + tools/superbuu/sbdriver.py. ('mark', name) saves a checkpoint state
work/superbuu/m_<name>.ss when reached (no frame used).

usage: sbroute.py [--from MARK] [--to MARK] [--cap N]"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent / 'work' / 'superbuu'   # states, key logs (local)
OUT.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent.parent / 'tools' / 'story_playthrough'))
import sbdriver  # noqa: E402,F401
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402

A, B, SEL, START, RIGHT, LEFT, UP, DOWN, R, L = (1 << i for i in range(10))

ROUTE = [
    ('mark', 'start'),
    # HFIL 18-13 (save point) -> 18-12 -> 18-1; Goku's pad (A) -> the Other World map 0-51: RIGHT to Yemma's Office
    ('walk', 24, 290), ('hold', DOWN, 30), ('area', 18, 12), ('wait', 30),
    ('walk', 136, 222), ('hold', DOWN, 30), ('area', 18, 1), ('wait', 30),
    ('walk', 185, 258), ('talk', 'D'), ('wait', 60), ('mark', 'ow_map'),
    ('hold', RIGHT, 40), ('wait', 10), ('hold', A, 3), ('areaw', 1, 5), ('wait', 30),
    # 1-5 -> 1-3: King Yemma (s059, flag 126) -> back to 1-5: s067 (127) -> Supreme Kai's world 19-3 / 19-7 (Gohan)
    ('walk', 336, 156), ('hold', UP, 30), ('area', 1, 3), ('wait', 40), ('walk', 232, 362), ('talk', 'U'),
    ('wait', 30), ('walk', 224, 450), ('hold', DOWN, 30), ('area', 1, 5), ('wait', 40), ('walke', 336, 200),
    ('events',), ('wait', 60), ('mark', 'zsword'),
    # 19-7: the Z-Sword (s464): mash to pull it, swing at the boulder / the Katchin 47 frames after the throw starts
    ('walk', 152, 96), ('hold', UP, 2), ('wait', 3), ('hold', A, 3), ('wait', 5), ('fastmash', 1200),
    ('node', 's464', 19, 47, A, 3), ('node', 's464', 33, 47, A, 3), ('events',), ('wait', 30), ('mark', 'lookout'),
    # Kami's Lookout 6-5 (Trunks): the bed (s224) -> HTC 6-9: spar with Goten (s225 / s226) -> 6-7
    ('walk', 120, 100), ('talk', 'R'), ('wait', 30), ('hold', A, 3), ('wait', 5), ('events',), ('wait', 30),
    ('boss', 12000), ('events',), ('wait', 30),
    # 6-7 -> 6-6 -> 6-9: Goten again: "No" to sparring -> s227: the fusion (Watch This!) -> Gotenks vs Super Buu
    ('walk', 8, 100), ('hold', LEFT, 20), ('area', 6, 6), ('wait', 30), ('walk', 108, 218), ('hold', DOWN, 20),
    ('area', 6, 9), ('wait', 30), ('walk', 192, 66), ('hold', DOWN, 2), ('wait', 3),
    ('node', 's225', 15, 20, DOWN, 3), ('prog', '0:10,A:3'), ('eventsw', 240), ('wait', 30), ('mark', 'gotenks'),
    ('boss', 20000), ('events',), ('wait', 30), ('mark', 'after_gotenks'),
    # 6-20: the crater's edge (s232, A) -> Giant Crater 6-16: Gotenks vs Super Buu (s233 / s234 / s235); s455: Gohan's
    # power awakened (Supreme Kai's world 19-3) -> Southwest Forest 8-1 (Ultimate Gohan)
    ('walk', 400, 512), ('talk', 'D'), ('wait', 30), ('boss', 30000), ('events',), ('wait', 30), ('mark', 'ult_gohan'),
    # 8-1 capsule (A) -> world map 0-50: fly to the Giant Crater spot (633, 1065) and land (inline_0x08073799 -> s234)
    ('walk', 280, 256), ('talk', 'U'), ('fly', 648, 1076), ('events',), ('wait', 30), ('mark', 'gohan_vs_buu'),
    # Ultimate Gohan vs Super Buu (6-16) -> Buu blows himself up (s466) -> Barren Wasteland 20-1
    ('boss', 40000), ('events',), ('wait', 30), ('mark', 'wasteland'),
]


def run(frm=None, to=None, cap=10 ** 7, verbose=True):
    goals = ROUTE
    st = OUT / 'sb_start.ss'
    if frm:
        i = goals.index(('mark', frm))
        goals = goals[i + 1:]
        st = OUT / f'm_{frm}.ss'
    if to:
        goals = goals[:goals.index(('mark', to)) + 1]
    e = start_emu(str(st))
    d = Driver([g for g in goals])
    f = 0
    prev = None

    def on_mark(e, name):
        e.save_file(str(OUT / f'm_{name}.ss'))
        print('mark', name, f, status(e), flush=True)
    d.on_mark = on_mark
    log = []
    while not d.done() and f < cap:
        k = d.keys(e)
        log.append(k)
        e.set_keys(k)
        e.run(1)
        f += 1
        s = status(e)
        key = (tuple(s['area']), s['scene'] > 0)
        if verbose and key != prev:
            print(f, d.k, d.goals[d.k] if not d.done() else None, s, flush=True)
        prev = key
    print('end', f, status(e))
    if not frm:
        import json
        (OUT / 'sb_keys.json').write_text(json.dumps(log))
    return e


if __name__ == '__main__':
    arg = lambda k, dflt=None: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else dflt
    e = run(arg('--from'), arg('--to'), int(arg('--cap', 10 ** 7)))
    e.screenshot(str(OUT / 'route_end.png'), 1)
