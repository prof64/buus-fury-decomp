"""Area graph from the content (exits: map triggers + area `exit` objects with a push direction) and the shortest
exit sequence between areas (for ShipDriver's ('nav', w, a) goal)."""
import json
from collections import deque
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]   # (frozen copy one level down: re/buu/ship/r1frozen)
CONTENT = ROOT / 'engine/target/tmp/wf-core-field-content'


@lru_cache(None)
def areas():
    return {a['key']: a for a in json.loads((CONTENT / 'world/areas.json').read_text())['areas']}


@lru_cache(None)
def exits(key, variant=1):
    """[(to_key, rect, push)] of area `key` (map exits: unconditional; area exits: any condition)"""
    a = areas()[key]
    m = a['maps'][min(max(variant, 1), len(a['maps'])) - 1]
    o = json.loads((CONTENT / 'maps' / m / 'objects.json').read_text())
    out = []
    for t in o.get('map_triggers', []):
        if t.get('type') == 'exit' and (t.get('condition') or {}).get('always') and t.get('to'):
            out.append((t['to']['key'], t['rect'], t.get('push')))
    for ar in o.get('areas', []):
        if ar['key'] != key:
            continue
        for t in ar['objects']:
            if t['type'] == 'exit' and t.get('to'):          # conditional exits too (story-gated doors)
                out.append((t['to']['key'], t['rect'], t.get('push')))
    return out


def route(src, dst, avoid=()):
    """the list of (area, exit) hops from src to dst (keys 'w-a')"""
    prev = {src: None}
    q = deque([src])
    while q:
        k = q.popleft()
        if k == dst:
            break
        for to, rect, push in exits(k):
            if to not in prev and to in areas() and to not in avoid:
                prev[to] = (k, (to, rect, push))
                q.append(to)
    if dst not in prev:
        return None
    hops = []
    k = dst
    while prev[k] is not None:
        k0, ex = prev[k]
        hops.append((k0, ex))
        k = k0
    return hops[::-1]


DIRS = {'up': (0, -1), 'down': (0, 1), 'left': (-1, 0), 'right': (1, 0)}


def approach(rect, push):
    """footprint top-left to walk to before pushing into the exit: the rect's centre, pulled back 12 px against the
    push direction (so the walk stays inside the room), clamped to >= 0"""
    cx = rect['x'] + rect['w'] // 2 - 8
    cy = rect['y'] + rect['h'] // 2 - 4
    dx, dy = DIRS.get(push, (0, 0))
    return max(cx - dx * 12, 0), max(cy - dy * 12, 0)


if __name__ == '__main__':
    import sys
    for h in route(sys.argv[1], sys.argv[2]):
        print(h[0], '->', h[1][0], h[1][2], approach(h[1][1], h[1][2]))
