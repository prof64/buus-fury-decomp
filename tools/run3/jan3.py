"""Run 3's Janemba chapter (`j3_janemba`; engine/docs/segments.md "Run 3"), recorded from the fat-Buu part's end
(work/run3fat/dbx.ss + .eeprom: 0-96, the Janemba prologue) with run 2's pieces (tools/run2/fat2.py j_*: their goals
and drivers, as patched by tools/run3/fat3.py) and fat3's stress interlude (three pause spins, the pause pages both
ways, START on alternate frames, the skill slot put back) before every piece but the first two (NO_INTERLUDE). Input only.

`python3 tools/run3/jan3.py --all [--from PIECE]` plays the pieces in order from the fat end (each from the previous
one's end) -> work/run3jan/<PIECE>.{ss,eeprom,keys.json}; `--part` joins them into work/run3/parts/j3_janemba.json."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT / 'tools' / 'run3') not in sys.path:
    sys.path.insert(0, str(ROOT / 'tools' / 'run3'))
import fat3  # noqa: E402  (patches fat2's drivers for run 3)
from fat3 import fat2, play, Interlude  # noqa: E402
from route import status  # noqa: E402

ORDER = ['j_intro', 'j_grind', 'j_hfil', 'j_maze', 'j_back', 'j_vegeta', 'j_gate', 'j_arena']
OUT = ROOT / 'work' / 'run3jan'
# (j_intro: the prologue runs into it; j_grind: it starts on 18-1's Instant Transmission pad - the interlude's
# presses there moved Goku and the grind's first walk stepped back onto the pad: the Other World map)
NO_INTERLUDE = ('j_intro', 'j_grind', 'j_maze')    # (j_maze: 18-10's Poltergeists drain Goku through the spins)
START = (ROOT / 'work' / 'run3fat' / 'dbx.ss', ROOT / 'work' / 'run3fat' / 'dbx.eeprom')


def hfil3(e):
    """run 2's j_hfil (Instant Transmission at 18-2, the corridor to 18-10) after a Save at 18-1's save point, under a
    Retry: run 3's Goku died on the way (18-3 .. 18-6) - a game over continues from that save and plays it again"""
    return fat2.Chain([
        lambda e: fat2.Driver2(fat2.save_at(18, 1)),
        lambda e: fat2.Retry([fat2.hfil_part('hfil', 'maze')],
                             [fat2.DEATH_CONTINUE, ('until', 18, 1), ('wait', 30)], tries=10),
    ])


def maze3(e):
    """run 2's j_maze (18-10's gate maze, the boulder, 18-11's Vegetas) under a Retry: run 3's Goku came in with less
    HP (764; run 2 1,079) and the Poltergeists drained him - a game over continues from 18-1's save (hfil3's), plays
    the corridor again (j_hfil's route) and then the maze"""
    # (a search 0 .. 23 from j_hfil's end, work/run3/scr/mazesearch.py: 4, 8, 10, 16, 17, 18 get through; 17 leaves
    # the most HP, 752)
    fat2.MAZE_PUNCHES = 17

    def attempt(e):
        maze = fat2.g_j_maze(e)
        if status(e)['area'][:2] == [18, 10]:
            return maze
        return fat2.Chain([fat2.hfil_part('hfil', 'maze'), lambda e: fat2.g_j_maze(e)])
    return fat2.Retry([attempt], [fat2.DEATH_CONTINUE, ('until', 18, 1), ('wait', 30)], tries=10)


GRIND_TO = 125


def grind3(e):
    """run 2's j_grind but to level GRIND_TO on 18-2's Living Dead (run 2: 116, run 1: 120): at 116 run 3's Goku died
    in the corridor 18-6 / 18-7 and the maze in ten tries running"""
    return fat2.Driver2([('grind', 92, [(18, 12), (18, 1)]), ('grind', GRIND_TO, [(18, 2), (18, 1)]),
                         fat2.nav(18, 1, False), ('wait', 30)])


_arena_part = fat2.arena_part


def arena_part3(wait):
    """fat2.arena_part with each boss fight ('bosst') followed by three more (play the events, fight) pairs: 'bosst'
    ends at the first event, and in run 3 Goku levelled up mid-fight against Janemba (the level-up message is an event:
    the route went on with Janemba alive); a repeat with none of its types alive ends after 30 frames"""
    def make(e):
        d = _arena_part(wait)(e)
        out = []
        for g in d.goals:
            out.append(g)
            if isinstance(g, tuple) and g and g[0] == 'bosst':
                # ('eventsw': also plays the Watch This! / power-struggle modes of Super Janemba's handover)
                out += [('eventsw', 240), g] * 3
        d.goals[:] = out
        return d
    return make


fat2.arena_part = arena_part3


def driver(name, e):
    if name == 'j_grind':
        return grind3(e)
    if name == 'j_hfil':
        return hfil3(e)
    if name == 'j_maze':
        return maze3(e)
    return fat3.driver3(name, e)


def one(name, start, eeprom, cap=600000):
    import sbemu
    from route import start_emu
    OUT.mkdir(parents=True, exist_ok=True)
    e = start_emu(str(Path(start).resolve()))
    sbemu.sd_write(e, Path(eeprom).read_bytes())
    keys = []
    try:
        if name not in NO_INTERLUDE:
            play(Interlude(), e, keys, cap, f'{name}/interlude')
        play(driver(name, e), e, keys, cap, name)
    except Exception as ex:  # noqa: BLE001
        e.save_file(str(OUT / f'{name}.fail.ss'))
        (OUT / f'{name}.fail.eeprom').write_bytes(sbemu.sd_read(e))
        e.screenshot(str(OUT / f'{name}.fail.png'), 2)
        (OUT / f'{name}.fail.keys.json').write_text(json.dumps(keys))
        print('FAIL', name, len(keys), ex, status(e), flush=True)
        return False
    e.set_keys(0)
    e.save_file(str(OUT / f'{name}.ss'))
    (OUT / f'{name}.eeprom').write_bytes(sbemu.sd_read(e))
    (OUT / f'{name}.keys.json').write_text(json.dumps(keys))
    print('done', name, len(keys), status(e), flush=True)
    return True


def main():
    a = sys.argv[1:]
    if a[0] == '--part':
        keys, sizes = [], {}
        for p in ORDER:
            k = json.loads((OUT / f'{p}.keys.json').read_text())
            keys += k
            sizes[p] = len(k)
        (ROOT / 'work' / 'run3' / 'parts' / 'j3_janemba.json').write_text(json.dumps(keys))
        print('j3_janemba', len(keys), sizes)
        return
    if a[0] == '--all':
        i0 = ORDER.index(a[a.index('--from') + 1]) if '--from' in a else 0
        st, ee = START if i0 == 0 else (OUT / f'{ORDER[i0 - 1]}.ss', OUT / f'{ORDER[i0 - 1]}.eeprom')
        for name in ORDER[i0:]:
            if not one(name, st, ee):
                sys.exit(1)
            st, ee = OUT / f'{name}.ss', OUT / f'{name}.eeprom'
        return
    sys.exit(__doc__)


if __name__ == '__main__':
    main()
