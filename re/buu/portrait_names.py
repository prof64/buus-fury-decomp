"""portrait_names.py [--apply] -- portrait names from the Capsule Corp database (row portrait field - 8 = portrait index)
and the playable forms (characters.json portrait byte - 8). --apply rewrites only the `portraits` section of
tools/extract/data/names_buu.json."""
import json
import sys
from collections import defaultdict
from common import DATA, ROOT
from tables import db_entries

NAMES = ROOT / 'tools' / 'extract' / 'data' / 'names_buu.json'
text = NAMES.read_text()
doc = json.loads(text)
por = doc['portraits']
cand = defaultdict(set)
for d in db_entries():
    if d['portrait'] >= 8:
        cand[d['portrait'] - 8].add(d['name'])
chars = json.loads((DATA / 'characters.json').read_text())
for f in chars['forms']:
    p = f['portrait_or_anim_bytes'][0] - 8
    if p >= 0 and f['name']:
        cand[p].add(('SS3 ' if f['flags'] & 2 else 'SS ' if f['flags'] & 1 else '') + f['name'])
out = {}
for k, v in sorted(por.items(), key=lambda kv: int(kv[0])):
    i = int(k)
    c = sorted(cand.get(i, []), key=lambda s: (s.startswith('Human'), len(s)))
    new = dict(v)
    old = v.get('name', '').replace('Super Saiyan 3 ', 'SS3 ').replace('Super Saiyan ', 'SS ')
    bases = [s.replace('SS3 ', '').replace('SS ', '').replace('#', '').lower() for s in c]
    if c and c[0] != 'Human':
        if i == 15:
            new['name'] = 'Criminal (2)'
        elif any(b in old.lower() for b in bases) or old == 'Great Saiyaman':   # Dojo text string 122
            new['name'] = old          # already the game name (+ its qualifier)
        else:
            new['name'] = c[0]
        new['confidence'] = 'high'
        new['source'] = 'Capsule Corp database / playable form portrait'
    if new != v:
        print(i, v.get('name'), '->', new.get('name'), sorted(c))
    out[k] = new
for i in sorted(set(cand) - {int(k) for k in por}):
    print('no entry for portrait', i, sorted(cand[i]))
if '--apply' in sys.argv:
    key = '\n "portraits": '
    a = text.index(key) + len(key)
    _, end = json.JSONDecoder().raw_decode(text, a)
    NAMES.write_text(text[:a] + json.dumps(out, indent=1, ensure_ascii=False).replace('\n', '\n ') + text[end:])
    print('wrote')
