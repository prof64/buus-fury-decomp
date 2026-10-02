"""Check that every asset the site references exists under site/public/.

Scans
  * data/**/*.json        every string value that looks like "assets/..."
  * site/src/**/*.ts(x)   every literal "assets/..." path (template parts `${...}` become a glob that must match >= 1 file)
  * the battle HUD icons  derived from data (LoG2 move ids, Buu skill ids), as the rulesets build those paths
and prints the missing ones. Exit status 1 if anything is missing.

usage: python3 tools/check_assets.py [-v]
"""
import glob
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PUBLIC = ROOT / 'site' / 'public'
VERBOSE = '-v' in sys.argv

missing: dict[str, str] = {}   # path -> first place it was referenced
checked = 0


def check(path: str, where: str):
    global checked
    path = path.lstrip('/')
    checked += 1
    if '*' in path:
        if not glob.glob(str(PUBLIC / path)):
            missing.setdefault(path, where)
    elif not (PUBLIC / path).is_file():
        missing.setdefault(path, where)


def walk(v, where):
    if isinstance(v, str):
        if re.match(r'^/?assets/.+\.\w+$', v):
            check(v, where)
    elif isinstance(v, list):
        for x in v:
            walk(x, where)
    elif isinstance(v, dict):
        for x in v.values():
            walk(x, where)


# 1. data json
for p in sorted((ROOT / 'data').rglob('*.json')):
    walk(json.loads(p.read_text()), str(p.relative_to(ROOT)))

# 2. code literals ('...', "...", `...`) that contain an assets/ path with a file extension
LIT = re.compile(r"""(['"`])(/?assets/(?:[^'"`\s]|\$\{[^}]*\})*?\.(?:png|gif|json|jpg|webp))\1""")
for p in sorted((ROOT / 'site' / 'src').rglob('*.ts*')):
    for m in LIT.finditer(p.read_text()):
        path = re.sub(r'\$\{[^}]*\}', '*', m.group(2))
        check(path, str(p.relative_to(ROOT)))

# 3. icons the rulesets build from data ids (see rulesets/log2/data.ts, rulesets/buu/data.ts)
for m in json.loads((ROOT / 'data/log2/moves.json').read_text())['player_moves']:
    for alt in ('n', 'alt'):
        check(f"assets/log2/battle/icons/move_{m['id']}_{alt}.png", 'log2 player_moves')
for s in json.loads((ROOT / 'data/buu/moves.json').read_text())['skills']:
    for alt in ('n', 'alt'):
        check(f"assets/buu/battle/icons/skill_{s['id']}_{alt}.png", 'buu skills')

print(f'{checked} asset references checked, {len(missing)} missing')
for path, where in sorted(missing.items())[: None if VERBOSE else 40]:
    print(f'  MISSING {path}   (from {where})')
if not VERBOSE and len(missing) > 40:
    print(f'  ... and {len(missing) - 40} more (-v for all)')
sys.exit(1 if missing else 0)
