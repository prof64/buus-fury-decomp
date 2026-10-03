"""where.py PATTERN... : find map objects whose string fields fully match PATTERN (e.g. s066)"""
import json, glob, sys, re
R = str(__import__('pathlib').Path(__file__).resolve().parents[2]) + '/'


def walk(o, path, hits, pat):
    if isinstance(o, dict):
        for k, v in o.items():
            walk(v, path + [k], hits, pat)
    elif isinstance(o, list):
        for i, v in enumerate(o):
            walk(v, path + [i], hits, pat)
    elif isinstance(o, str) and re.fullmatch(pat, o):
        hits.append(path)


def find(pat):
    out = []
    for f in sorted(glob.glob(R + 'work/field_content/maps/*/objects.json')):
        d = json.load(open(f))
        hits = []
        walk(d, [], hits, pat)
        for h in hits:
            o = d
            for k in h[:-1]:
                o = o[k]
            area = d['areas'][h[1]]['key'] + ' v' + str(d['areas'][h[1]].get('variant')) if h[0] == 'areas' else '-'
            out.append((d['map'], area, {k: o.get(k) for k in ('type', 'condition', 'rect', 'pos', 'event', 'to')}))
    return out


if __name__ == '__main__':
    for pat in sys.argv[1:]:
        for r in find(pat):
            print(pat, *r)
