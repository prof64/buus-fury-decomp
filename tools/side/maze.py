"""Walk-region planner over a group of areas whose exits join several separate floors (HFIL 18-x: cliffs, edge exits
that keep the coordinate along the edge).

Nodes are (area key, component id) - the connected components of the footprint-walkable tiles (2 x 1 tiles); a portal
is a standing tile next to / inside an exit rect together with where it lands. plan(src_key, (x, y), dst_key, (x, y))
returns the hops [(area, component, [standing tiles], push, exit rect, dest area)]."""
import json
import sys
from collections import deque
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from grid import Grid, map_of  # noqa: E402
import nav as navm  # noqa: E402

DIRS = {'up': (0, -1), 'down': (0, 1), 'left': (-1, 0), 'right': (1, 0)}
# walls a switch opens (tiles that become walkable), by area: add before planning once the switch is done
# (HFIL: 18-2's Instant Transmission switch, flag 181, hides 18-4's wall x 88..151, y 256)
OPEN = {}


def open_wall(key, tiles):
    OPEN[key] = OPEN.get(key, []) + list(tiles)
    area.cache_clear()
    portals.cache_clear()


@lru_cache(None)
def area(key):
    w, a = (int(v) for v in key.split('-'))
    m = map_of(w, a, 1)
    g = Grid(m)
    for tx, ty in OPEN.get(key, ()):
        g.walk[ty * g.w + tx] = 1
    comp = {}
    n = 0
    for ty in range(g.h):
        for tx in range(g.w):
            if (tx, ty) in comp or not g.ok_box(tx, ty):
                continue
            q = deque([(tx, ty)])
            comp[(tx, ty)] = n
            while q:
                cx, cy = q.popleft()
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    c = (cx + dx, cy + dy)
                    if c not in comp and g.ok_box(*c):
                        comp[c] = n
                        q.append(c)
            n += 1
    o = json.loads((navm.CONTENT / 'maps' / m / 'objects.json').read_text())
    trig = [t for t in o.get('map_triggers', []) if t.get('type') in ('exit', 'edge_exit') and t.get('to')]
    return g, comp, trig


def comp_at(key, x, y):
    """the component of the footprint at px (x, y), or of the nearest walkable tile within 3 tiles"""
    _, comp, _ = area(key)
    t = (x // 8, y // 8)
    if t in comp:
        return comp[t]
    for rad in (1, 2, 3):
        best = None
        for dx in range(-rad, rad + 1):
            for dy in range(-rad, rad + 1):
                c = (t[0] + dx, t[1] + dy)
                if c in comp and (best is None or abs(dx) + abs(dy) < best[0]):
                    best = (abs(dx) + abs(dy), comp[c])
        if best:
            return best[1]
    return None


@lru_cache(None)
def portals(key):
    """[(component, standing tile, push, rect, dest key, dest component)]"""
    g, comp, trig = area(key)
    out = []
    for t in trig:
        r, push = t['rect'], t.get('push')
        dx, dy = DIRS.get(push, (0, 0))
        to = t['to']
        if to['key'] not in navm.areas():
            continue
        # standing tiles: the footprint (16 x 8 at the tile) one 8-px step before entering the rect
        x0, y0, x1, y1 = r['x'], r['y'], r['x'] + r['w'], r['y'] + r['h']
        for (tx, ty), c in comp.items():
            fx0, fy0 = tx * 8, ty * 8
            # (the trigger tests the player's position, the footprint's top-left px, against the rect)
            ins = x0 <= fx0 < x1 and y0 <= fy0 < y1
            nx0, ny0 = fx0 + dx * 8, fy0 + dy * 8
            nxt = x0 <= nx0 < x1 and y0 <= ny0 < y1
            if not nxt or ins:
                continue
            if to.get('relative'):
                # an edge exit keeps the offset along the edge: dest = to + (position - rect origin)
                ax, ay = (to['x'], to['y'] + fy0 - r['y']) if dx else (to['x'] + fx0 - r['x'], to['y'])
            else:
                ax, ay = to['x'], to['y']
            dc = comp_at(to['key'], ax, ay)
            if dc is None:
                continue
            out.append((c, (tx, ty), push, r, to['key'], dc))
    return out


def plan(src, sxy, dst, dxy):
    s = (src, comp_at(src, *sxy))
    d = (dst, comp_at(dst, *dxy))
    prev = {s: None}
    q = deque([s])
    while q:
        n = q.popleft()
        if n == d:
            break
        for c, tile, push, rect, to, dc in portals(n[0]):
            if c != n[1]:
                continue
            m = (to, dc)
            if m not in prev:
                prev[m] = n
                q.append(m)
    if d not in prev:
        return None
    path = [d]
    while prev[path[-1]] is not None:
        path.append(prev[path[-1]])
    path.reverse()
    hops = []
    for a, b in zip(path, path[1:]):
        tiles = [(t, push, rect) for c, t, push, rect, to, dc in portals(a[0]) if c == a[1] and (to, dc) == b]
        hops.append((a[0], a[1], tiles, b[0]))
    return hops


if __name__ == '__main__':
    a = sys.argv
    h = plan(a[1], (int(a[2]), int(a[3])), a[4], (int(a[5]), int(a[6])))
    for x in h or []:
        print(x[0], 'comp', x[1], '->', x[3], len(x[2]), 'tiles e.g.', x[2][0][0], x[2][0][1])
    if h is None:
        print('no route')
