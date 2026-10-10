"""Print the key structure of a reference JSON (list items merged)."""
import json
import sys


def walk(v, path, out, depth):
    if isinstance(v, dict):
        for k, x in v.items():
            walk(x, f'{path}.{k}', out, depth + 1)
        if not v:
            out.setdefault(path, set()).add('{}')
    elif isinstance(v, list):
        if not v:
            out.setdefault(path, set()).add('[]')
        for x in v[:400]:
            walk(x, path + '[]', out, depth + 1)
    else:
        s = repr(v)
        out.setdefault(path, set()).add(s[:60])


d = json.load(open(sys.argv[1]))
out = {}
walk(d, '', out, 0)
maxd = int(sys.argv[2]) if len(sys.argv) > 2 else 99
for k, v in out.items():
    if k.count('.') <= maxd:
        print(k, '|', len(v), '|', ' ; '.join(sorted(v)[:3]))
