"""HFIL 18-10's gate maze: two wall states toggled by the centre switch (Instant Transmission from anywhere the
cursor reaches). plan(start_px, goal_px, state) -> [('walk', x, y) | ('toggle',)] over (state, walk component)."""
import sys
from collections import deque
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from grid import Grid, map_of  # noqa: E402

# live-grid difference of state 1 against the map's own grid (state 0): (tx, ty, walkable)
DIFF = [(36, 35, 0), (37, 35, 0), (38, 35, 0), (39, 35, 0), (41, 35, 1), (42, 35, 1), (43, 35, 1), (44, 35, 1),
        (34, 36, 1), (46, 36, 0), (34, 37, 1), (46, 37, 0), (34, 38, 1), (46, 38, 0), (34, 39, 1), (46, 39, 0),
        (34, 41, 0), (46, 41, 1), (34, 42, 0), (46, 42, 1), (34, 43, 0), (46, 43, 1), (34, 44, 0), (46, 44, 1),
        (36, 46, 1), (37, 46, 1), (38, 46, 1), (39, 46, 1), (41, 46, 0), (42, 46, 0), (43, 46, 0), (44, 46, 0)]
# the spike strips (script 507: 20 HP and a 3 px knock-back): rects in px
SPIKES = [(65, 25, 175, 39), (433, 25, 111, 39), (569, 121, 47, 391), (113, 585, 399, 39), (17, 409, 44, 135),
          (17, 89, 44, 155)]
SWITCH = (317, 320)
REACH = 110     # px the cursor is steered at most (the switch must be on screen)


def forbidden(g, rects):
    """the spike triggers test the player's position (the footprint's top-left px) against the rect: the tile
    positions (footprint origins) inside one"""
    out = set()
    for x, y, w, h in rects:
        for ty in range(max(0, y // 8), min(g.h, (y + h) // 8 + 1)):
            for tx in range(max(0, x // 8), min(g.w, (x + w) // 8 + 1)):
                if x <= tx * 8 < x + w and y <= ty * 8 < y + h:
                    out.add((tx, ty))
    return out


def grids():
    out = []
    for st in (0, 1):
        g = Grid(map_of(18, 10, 1))
        if st:
            for tx, ty, v in DIFF:
                g.walk[ty * g.w + tx] = v
        out.append(g)
    return out


def plan(start, goal, state=0, spikes=True):
    gs = grids()
    bad = forbidden(gs[0], SPIKES) if spikes else set()
    s = (state, start[0] // 8, start[1] // 8)
    gl = (goal[0] // 8, goal[1] // 8)
    prev = {s: None}
    q = deque([s])
    end = None
    while q:
        n = q.popleft()
        st, tx, ty = n
        if (tx, ty) == gl:
            end = n
            break
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            m = (st, tx + dx, ty + dy)
            if m not in prev and gs[st].ok_box(m[1], m[2]) and (m[1], m[2]) not in bad:
                prev[m] = n
                q.append(m)
        # the toggle: where the cursor reaches the switch and the footprint is free in the other state too
        if abs(tx * 8 - SWITCH[0]) + abs(ty * 8 - SWITCH[1]) <= REACH and gs[1 - st].ok_box(tx, ty):
            m = (1 - st, tx, ty)
            if m not in prev:
                prev[m] = n
                q.append(m)
    if end is None:
        return None
    path = [end]
    while prev[path[-1]] is not None:
        path.append(prev[path[-1]])
    path.reverse()
    goals = []
    for a, b in zip(path, path[1:]):
        if a[0] != b[0]:
            goals.append(('walk', a[1] * 8, a[2] * 8))
            goals.append(('toggle',))
    goals.append(('walk', goal[0], goal[1]))
    return goals


if __name__ == '__main__':
    a = [int(v) for v in sys.argv[1:6]]
    print(plan((a[0], a[1]), (a[2], a[3]), a[4]))
    print(plan((a[0], a[1]), (a[2], a[3]), a[4], False))
