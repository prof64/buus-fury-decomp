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
else:
    raise SystemExit(f'no route {name}')
db_route.main(m.ROUTES, drv)
