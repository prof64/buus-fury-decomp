"""enemies_of.py W A0 A1: the enemies placed in areas W-A0 .. W-A1 (type, name, level, HP, position, condition)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
C = ROOT / 'engine/target/tmp/wf-core-field-content'


def table():
    for p in (C / 'enemies' / 'index.json', C / 'enemies.json', ROOT / 'data/buu/enemies.json'):
        if p.exists():
            d = json.loads(p.read_text())
            return d.get('enemies', d) if isinstance(d, dict) else d
    return None


def main():
    w, a0, a1 = map(int, sys.argv[1:4])
    T = table()
    for a in range(a0, a1 + 1):
        p = C / 'maps' / f'area_{w:02d}_{a:03d}' / 'objects.json'
        if not p.exists():
            continue
        o = json.loads(p.read_text())
        for ar in o['areas']:
            for t in ar['objects']:
                if t['type'] != 'enemy':
                    continue
                ty = t.get('enemy_type', t.get('enemy'))
                info = ''
                if T is not None and isinstance(ty, int) and ty < len(T):
                    x = T[ty]
                    info = {k: x[k] for k in x if k in ('name', 'level', 'hp', 'str', 'xp')}
                print(f'{w}-{a}', ty, info, t.get('pos'), 'sprite', t.get('sprite'), t.get('condition') or '')


if __name__ == '__main__':
    main()
