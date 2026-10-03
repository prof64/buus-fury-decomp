"""mapgraph.py FROM TO  (keys like 18-13 1-3): BFS over the exits of work/field_content/maps/*/objects.json"""
import json, glob, sys, collections
R = str(__import__('pathlib').Path(__file__).resolve().parents[2]) + '/'


def load():
    G = collections.defaultdict(list)
    names = {}
    for f in sorted(glob.glob(R + 'work/field_content/maps/*/objects.json')):
        d = json.load(open(f))
        w, a = d['map'].split('_')[1:3]
        k = f'{int(w)}-{int(a)}'
        objs = [(o, None) for o in d.get('map_triggers', [])]
        for ar in d['areas']:
            names[k] = ar.get('name')
            objs += [(o, ar.get('variant')) for o in ar['objects']]
        for o, v in objs:
            if o.get('type') in ('exit', 'edge_exit') and o.get('to'):
                t = o['to']
                G[k].append((f"{t['world']}-{t['area']}", o.get('rect'), (t.get('x'), t.get('y')), o.get('condition'), o.get('type'), v, o.get('push')))
    return G, names


def path(G, a, b):
    prev = {a: None}
    q = collections.deque([a])
    while q:
        u = q.popleft()
        if u == b:
            break
        for e in G[u]:
            if e[0] not in prev:
                prev[e[0]] = (u, e)
                q.append(e[0])
    if b not in prev:
        return None
    out = []
    while prev[b]:
        u, e = prev[b]
        out.append((u, e))
        b = u
    return out[::-1]


if __name__ == '__main__':
    G, names = load()
    if len(sys.argv) == 2:
        for e in G[sys.argv[1]]:
            print(e)
    else:
        for u, e in path(G, sys.argv[1], sys.argv[2]) or []:
            print(u, names.get(u), '->', e)
