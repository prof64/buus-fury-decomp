"""Flying Mega-Fortress XJ-6 (post-game airship, world 12) from the REAL post-game state: pgroute leg `xj6a` flies
to the airship marker (flag 149, not 151) -> 12-1; this script then plays the dungeon agent B's Airship goals
(tools/db57: DbDriver, the same 12-x rooms as XJ-5) with the post-game's own events: s385 in 12-2 (the cyborg,
flag 150), the lift to 12-28 (s388: the second Warlord), the fight, s389 (the Hercule Exhibit, flag 151, then the
countdown, flag 95), back out to 12-1's pad, the world map.

    .venv/bin/python tools/side/xj6.py            -> work/side/pg_xj6.ss + pg_xj6.keys.json ({'from', 'keys'})
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / 'tools' / 'db57')]
from dbdriver import DbDriver, KEY, status  # noqa: E402
from route import start_emu  # noqa: E402

D = ROOT / 'work' / 'side'
GOALS = [
    ('go', 12, 27, 'near'),
    # the lift: s388 (the Warlord again) - the trigger rect (104, 344, 17, 9) wants x >= 104: walk (108, 358), then UP
    # (the db5 route's `trig` stood 1 px left of it; segments.md §8 a02_warlord)
    ('walk', 108, 358), ('hold', KEY.UP, 40), ('events',), ('wait', 120),
    # (the boss, an Ultra Bio Mech (0x83, 8,945 HP), stands idle at (157, 97) until the player is within 80 px: walk
    # up to it first - `clear` alone ends before it is engaged)
    ('clear', 20000), ('walk', 150, 140), ('clear', 30000),
    ('events',),
    ('go', 12, 1, 'near'),
    ('trig', 32, 124, KEY.A),               # the pad -> the world map
    ('wait', 120),
]
e = start_emu(str(D / 'pg_xj6a.ss'))
d = DbDriver(list(GOALS))
d.on_mark = lambda *a: None
keys = []
while not d.done():
    try:
        k = d.keys(e)
    except RuntimeError as ex:
        print('failed', ex, d.k, status(e))
        break
    e.set_keys(k)
    e.run(1)
    keys.append(k)
    if len(keys) % 2000 == 0:
        print(len(keys), d.k, status(e), flush=True)
    if len(keys) > 80000 or status(e)['area'][:2] == [0, 2]:
        print('stopped', d.k, status(e))
        break
e.set_keys(0)
e.save_file(str(D / 'pg_xj6.ss'))
(D / 'pg_xj6.keys.json').write_text(json.dumps({'from': 'pg_xj6a.ss', 'keys': keys}))
print('end', len(keys), status(e))
