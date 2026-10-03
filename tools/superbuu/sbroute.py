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
    ('walk', 152, 90), ('hold', UP, 2), ('wait', 3), ('hold', A, 3), ('wait', 5), ('fastmash', 1200),
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
    # (beams from 70 px: Electric Kamehameha, slot 2, held 120 frames - melee loses to Buu's counters)
    ('beam', 40000, 2, 120, 70), ('events',), ('wait', 30), ('mark', 'wasteland'),
    # Barren Wasteland (Gohan): 20-2 (s467 / s468: Hercule and Bee) -> 20-3 -> 20-4 (Gohan Lv 140 gate, A) -> 20-5:
    # s469 (Dende; Gotenks' fusion, Watch This!; Buu absorbs Gotenks and Piccolo)
    ('exitto', '20-2'), ('expect', 300), ('wait', 20), ('walke', 150, 184), ('events',), ('wait', 20),
    ('exitto', '20-3'), ('wait', 20), ('exitto', '20-4'), ('wait', 20), ('walk', 368, 82), ('talk', 'U'), ('wait', 30),
    ('exitto', '20-5'), ('wait', 20), ('walke', 200, 240), ('eventsw', 240), ('wait', 20), ('mark', 'gohan_vs_buu2'),
    # Gohan vs Super Buu (118, defeat -> s470): Goku arrives (s471), Vegeta (s474..s476: Vegito), Vegito inside Buu,
    # Vegeta and Goku leave (s481, flag 137) -> 20-5 (Goku)
    ('boss', 40000), ('eventsw', 240), ('wait', 30), ('mark', 'fusion_saga'),
    # Fusion Saga (Goku): 20-6 (clear the robots; Goku Lv 140 gate s473: "confront Majin Buu? Yes") -> 20-7: Goku and
    # Vegeta vs Super Buu 0x77: three phase hits = Instant Transmission punches while he glides toward the screen's
    # bottom (scripts #474 / #475 / #476: the Potara, Vegito)
    ('exitto', '20-6'), ('wait', 5), ('boss', 12000), ('walk', 96, 44), ('talk', 'U'), ('eventsw', 240), ('wait', 20),
    ('mark', 'goku_vs_buu'),
    ('itbuu', 20000, 2, 176, 150, 119), ('eventsw', 240), ('itbuu', 20000, 2, 176, 150, 119), ('eventsw', 240),
    ('itbuu', 20000, 2, 176, 150, 119), ('eventsw', 240), ('wait', 30), ('mark', 'vegito'),
    # Vegito vs Super Buu (0x78, defeat -> s477), the absorption -> inside Buu 21-1 (Vegeta)
    ('boss', 12000), ('eventsw', 240), ('wait', 30), ('mark', 'inside_buu'),
    # inside Buu (Vegeta): 21-1's sphincter (A at its trigger: walk_to) -> 21-2 .. 21-6: break the pod (flag 139, s483)
    ('walk', 500, 300), ('talk', 'R'), ('wait', 30), ('exitto', '21-2'), ('wait', 10), ('exitto', '21-3'), ('wait', 10),
    ('exitto', '21-4'), ('wait', 10), ('exitto', '21-5'), ('wait', 10), ('exitto', '21-6'), ('wait', 10),
    ('walk', 112, 66), ('hold', UP, 2), ('prog', 'A:2,0:25,A:2,0:25,A:2,0:25,A:2,0:25'), ('events',), ('wait', 20),
    # -> 21-11: the three pods (140..142 -> s484: 143)
    ('exitto', '21-5'), ('wait', 10), ('exitto', '21-4'), ('wait', 10), ('exitto', '21-7'), ('wait', 10),
    ('exitto', '21-8'), ('wait', 10), ('exitto', '21-9'), ('wait', 10), ('exitto', '21-10'), ('wait', 10),
    ('exitto', '21-11'), ('wait', 10),
    ('walk', 104, 66), ('hold', UP, 2), ('prog', 'A:2,0:25,A:2,0:25,A:2,0:25,A:2,0:25'), ('events',), ('wait', 20),
    ('walk', 200, 66), ('hold', UP, 2), ('prog', 'A:2,0:25,A:2,0:25,A:2,0:25,A:2,0:25'), ('events',), ('wait', 20),
    ('walk', 296, 66), ('hold', UP, 2), ('prog', 'A:2,0:25,A:2,0:25,A:2,0:25,A:2,0:25'), ('events',), ('wait', 20), ('mark', 'intestines'),
    # -> 21-19 / 21-21 / 21-23: the captives (s485 / s486 / s487: Buu's fakes) -> 21-26 the brain: s488, Super Buu
    ('exitto', '21-10'), ('wait', 10), ('exitto', '21-9'), ('wait', 10), ('exitto', '21-8'), ('wait', 10),
    ('exitto', '21-12'), ('wait', 10), ('exitto', '21-13'), ('wait', 10), ('exitto', '21-14'), ('wait', 10),
    # 21-14 save point: Switch Characters -> Goku (full HP; Vegeta recovers in reserve)
    ('walk', 166, 236), ('prog', 'U:2,0:10,A:2,0:60,D:2,0:20,A:2,0:90,L:2,0:20,A:2,0:150'),
    ('exitto', '21-15'), ('wait', 10), ('exitto', '21-16'), ('wait', 10), ('exitto', '21-17'), ('wait', 10),
    ('exitto', '21-18'), ('wait', 10), ('exitto', '21-19'), ('wait', 10), ('walke', 100, 240), ('events',), ('wait', 20),
    ('boss', 20000), ('events',), ('wait', 20),
    ('exitto', '21-20'), ('wait', 10), ('exitto', '21-21'), ('wait', 10), ('walke', 100, 240), ('events',),
    ('boss', 20000), ('events',), ('wait', 20), ('exitto', '21-22'), ('wait', 10), ('exitto', '21-23'), ('wait', 10),
    ('walke', 100, 240), ('events',), ('boss', 20000), ('events',), ('wait', 20),
    ('exitto', '21-24'), ('wait', 10), ('exitto', '21-25'), ('wait', 10), ('exitto', '21-26'), ('wait', 10),
    ('walke', 200, 320), ('eventsw', 240), ('wait', 20), ('mark', 'brain'),
    # Super Buu (121; defeat s489: flag 147, Kid Buu Saga) -> s478 (out of Buu; s479 flag 148) -> world map
    ('boss', 30000), ('eventsw', 240), ('wait', 20), ('walke', 160, 40), ('eventsw', 240), ('wait', 30),
    ('mark', 'kid_buu_saga'),
    # Kid Buu Saga: fly back to the crater spot (s480: "ready to confront Kid Buu? Yes") -> Planet of the Kais 19-3: Goku
    # vs Kid Buu (splits, 125000 HP each round), Vegeta's timed stand (keep away from him until the countdown ends),
    # the Spirit Bomb (power struggle), flag 149 -> world map
    ('fly', 648, 1076), ('eventsw', 240), ('wait', 30), ('mark', 'kid_buu'),
    ('boss', 60000, 'timer'), ('eventsw', 240), ('evade', 20000), ('evade', 20000), ('eventsw', 240), ('wait', 30),
    ('mark', 'after_kid_buu'),
    # the final sequence: West City (world map spot 1250, 1200) -> Capsule Corp 4-20 -> 4-21: s190 ("final sequence of
    # the game ... ready? Yes") -> the party, the World Tournament, Uub (s311 / s312), the staff credits, the title
    ('fly', 1250, 1200), ('eventsw', 240), ('wait', 30), ('exitto', '4-20'), ('wait', 20), ('exitto', '4-21'),
    ('wait', 20), ('mark', 'final'), ('walke', 100, 76), ('eventsw', 240), ('wait', 30), ('mark', 'the_end'),
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
