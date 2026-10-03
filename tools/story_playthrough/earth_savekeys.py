"""Write data/story_playthrough/earth_route_keys.json: the key runs from the soft reset to the Earth chapter start
(route.py's intro + goals, checked against work/earth/chapter_start.ss) and the chapter's own keys
(work/earth/keys_all.json from earth_keys.py), so the chapter can be rebuilt without the local save states.

usage: python3 tools/story_playthrough/earth_savekeys.py"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import route  # noqa: E402
from driver import Driver  # noqa: E402
from route import start_emu, intro_keys, status, INTRO_FRAMES  # noqa: E402


def rle(ks):
    out = []
    for k in ks:
        if out and out[-1][0] == k:
            out[-1][1] += 1
        else:
            out.append([k, 1])
    return out


def main():
    e = start_emu('reset')
    keys = []
    for f in range(INTRO_FRAMES):
        k = intro_keys(f)
        e.set_keys(k)
        e.run(1)
        keys.append(k)
    d = Driver(route.ROUTE_GOALS)
    while not d.done():
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        keys.append(k)
    ref = start_emu(str(ROOT / 'work' / 'earth' / 'chapter_start.ss'))
    same = status(e) == status(ref) and e.ewram() == ref.ewram() and e.iwram() == ref.iwram()
    print('start state', status(e), 'identical to chapter_start.ss' if same else 'DIFFERS from chapter_start.ss')
    if not same:
        sys.exit(1)
    chap = json.loads((ROOT / 'work' / 'earth' / 'keys_all.json').read_text())
    doc = {'note': 'Earth chapters route (tools/story_playthrough/earth_*.py): start = keys after the soft reset of '
                   'work/states/buu_menus.ss (route.start_emu("reset")) to Earth 2-1 (New Game -> Other World), '
                   'chapter = keys from there to Goku in 7-2 after the World Tournament (flag 50); [key, frames] runs',
           'start': rle(keys), 'chapter': rle(chap)}
    out = ROOT / 'data' / 'story_playthrough' / 'earth_route_keys.json'
    out.write_text(json.dumps(doc, separators=(',', ':')))
    print(out, len(keys), '+', len(chap), 'frames')


if __name__ == '__main__':
    main()
