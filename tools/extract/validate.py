"""Sanity checks for data/<game>/sprites.json against the exported PNGs."""
import json
import sys
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent.parent


def check(game):
    doc = json.loads((ROOT / 'data' / game / 'sprites.json').read_text())
    errs = 0
    nframes = 0
    for e in doc['entities']:
        sheet = ROOT / 'site' / 'public' / e['sheet']
        if not sheet.exists():
            print(game, e['id'], 'missing sheet'); errs += 1; continue
        sw, sh = Image.open(sheet).size
        for i, f in enumerate(e['frames']):
            nframes += 1
            if f['x'] + f['w'] > sw or f['y'] + f['h'] > sh:
                print(game, e['id'], 'frame', i, 'outside sheet'); errs += 1
            if f.get('png') and not (ROOT / 'site' / 'public' / f['png']).exists():
                print(game, e['id'], 'missing png', f['png']); errs += 1
        for an, a in e['animations'].items():
            for d, seq in a['directions'].items():
                for s in seq:
                    if not 0 <= s['frame'] < len(e['frames']):
                        print(game, e['id'], an, d, 'bad frame ref'); errs += 1
    for m in doc.get('maps', []):
        if 'composite' in m and not (ROOT / 'site' / 'public' / m['composite']).exists():
            print(game, 'missing map', m['composite']); errs += 1
    cats = {}
    for e in doc['entities']:
        cats[e['category']] = cats.get(e['category'], 0) + 1
    print(game, 'entities', len(doc['entities']), 'frames', nframes, cats, 'maps',
          sum(1 for m in doc.get('maps', []) if 'composite' in m), 'errors', errs)
    return errs


if __name__ == '__main__':
    sys.exit(sum(check(g) for g in (sys.argv[1:] or ['log1', 'log2', 'buu'])) > 0)
