"""Walk grids from the extracted content (engine/docs/world.md collision.json) and a tile BFS for route planning."""
import base64
import json
from collections import deque
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CONTENT = ROOT / 'engine' / 'target' / 'tmp' / 'wf-core-field-content'


def leb(b, i):
    v = s = 0
    while True:
        x = b[i]
        i += 1
        v |= (x & 0x7F) << s
        s += 7
        if not x & 0x80:
            return v, i


def decode(s, n):
    b = base64.b64decode(s)
    out = bytearray()
    i, val = 0, 0          # first run = blocked
    while i < len(b) and len(out) < n:
        r, i = leb(b, i)
        out += bytes([val]) * r
        val ^= 1
    out += bytes([val]) * (n - len(out))
    return out


def areas():
    return json.loads((CONTENT / 'world' / 'areas.json').read_text())['areas']


def map_of(world, area, variant=1):
    a = next(a for a in areas() if a['world'] == world and a['area'] == area)
    maps = a['maps']
    return maps[min(max(variant, 1), len(maps)) - 1]


class Grid:
    def __init__(self, mapname):
        c = json.loads((CONTENT / 'maps' / mapname / 'collision.json').read_text())
        self.w, self.h = c['width'], c['height']
        self.walk = decode(c['walk'], self.w * self.h)
        self.objects = json.loads((CONTENT / 'maps' / mapname / 'objects.json').read_text())

    def ok(self, tx, ty):
        return 0 <= tx < self.w and 0 <= ty < self.h and self.walk[ty * self.w + tx] == 1

    def ok_box(self, tx, ty, fw=2, fh=1):
        return all(self.ok(tx + a, ty + b) for a in range(fw) for b in range(fh))

    def bfs(self, start, goal, fw=2, fh=1, blocked=()):
        """tile path from start to goal (tile coords of the footprint's top-left)"""
        bl = set(blocked)
        prev = {start: None}
        q = deque([start])
        while q:
            p = q.popleft()
            if p == goal:
                break
            for d in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)):
                n = (p[0] + d[0], p[1] + d[1])
                if n in prev or n in bl or not self.ok_box(n[0], n[1], fw, fh):
                    continue
                if d[0] and d[1] and not (self.ok_box(p[0] + d[0], p[1], fw, fh) and self.ok_box(p[0], p[1] + d[1], fw, fh)):
                    continue
                prev[n] = p
                q.append(n)
        if goal not in prev:
            return None
        path = []
        p = goal
        while p is not None:
            path.append(p)
            p = prev[p]
        return path[::-1]

    def show(self, x0=0, y0=0, x1=None, y1=None, marks=()):
        x1 = x1 or self.w
        y1 = y1 or self.h
        m = dict(marks)
        for ty in range(y0, y1):
            print(''.join(m.get((tx, ty), '.' if self.ok(tx, ty) else '#') for tx in range(x0, x1)))
