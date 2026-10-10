"""fat_db.py NAME START.ss [-v] [--from MARK] [--to MARK]  -- play a dungeon agent's route (tools/dungeons:
db2 Diablo Desert, db4 Nataday Village) by the real game from the REAL route's state START.ss, into
work/ship/fat/NAME/ (start.ss, <mark>.ss, end.ss, keys.json: the same layout as the agent's own work/db/NAME)."""
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
import db_route  # noqa: E402

name, start = sys.argv[1], sys.argv[2]
OUT = ROOT / 'work' / 'ship' / 'fat'
db_route.OUT = OUT
if '--from' not in sys.argv:
    shutil.copyfile(start, OUT / f'{name}_start.ss')
sys.argv = [sys.argv[0], name] + sys.argv[3:]
if name == 'db2':
    import db2_route as m
    from db2_driver import Db2Driver as drv
elif name == 'db4':
    import db4_route as m
    from db4_driver import Db4Driver as drv
elif name == 'db4b':
    # db4's way back from the Volcano (mark j_down) for the real Goten (level 115 by then, no Turkeys): his 18 new
    # stat points first, and no flame / block guard (two Destroyers in the open, 15-17: standing behind a guard
    # costs more than running through; the rooms' fights give the level-ups that heal)
    import db4_route as m
    from db4_driver import Db4Driver
    from ship_driver import alloc_keys

    class drv(Db4Driver):
        def flame_guard(self, e, s):
            return None

        def block_guard(self, e, s):
            return None

        def in_flame(self, e, px, py, margin=44):
            return False

    def db4b(e):
        gl = m.db4(e)
        i = gl.index(('mark', 'j_down'))
        return gl[:i + 1] + [('wait', 10), alloc_keys([(0, [9, 0, 9, 0])]), ('wait', 30)] + gl[i + 1:]
    m.db4b = db4b
else:
    raise SystemExit(f'no route {name}')
db_route.main(getattr(m, 'ROUTES', None) or {name: getattr(m, name)}, drv)
