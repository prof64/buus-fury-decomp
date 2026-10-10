"""run_tours.py NAME [NAME...]: record field coverage tours (segments tour_NAME in $SEGMENT_DIR, default
work/segs_tours). Tours come from every tools/segments/tour_goals*.py module's TOURS = {name: (state, goals)}
(state: a name under work/states or a path; goals: tours.py's driver goals)."""
import importlib
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import tours  # noqa: E402

TOURS = {}
for f in sorted(HERE.glob('tour_goals*.py')):
    TOURS.update(importlib.import_module(f.stem).TOURS)

if __name__ == '__main__':
    names = sys.argv[1:]
    if not names or names == ['--list']:
        print('\n'.join(sorted(TOURS)))
        sys.exit(0)
    for n in names:
        st, g = TOURS[n]
        tours.record(st, f'tour_{n}', g, 20000)
