"""Driver for the Dragon Ball dungeons 5-7 (on re/buu/ship/ship_driver.ShipDriver; deterministic closed loop).

Adds to ShipDriver's goals:
  ('go', w, a[, fight])   ShipDriver's nav, over a graph that also has the script-trigger warps (the Airship's lifts:
                          UP held on the pad; touch triggers), done 20 frames after arriving
  ('clear', cap)          fight everything reachable in the room (bosses too: no skipping), play events through
  ('item',)               walk onto the area's map item and play its event
  ('trig', x, y, key)     walk to (x, y), hold `key` until an event starts, play it through
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from ship_driver import ShipDriver, enemies, status, KEY  # noqa: E402,F401
import nav as navm  # noqa: E402

_plain_exits = navm.exits.__wrapped__ if hasattr(navm.exits, '__wrapped__') else navm.exits
UP_HELD = '0x080071A7'
_cache = {}
BLOCK = set()        # (from, to) hops a route must not take (set by a route module)


def exits(key, variant=1, gated=False):
    ck = (key, variant, gated)
    if ck in _cache:
        return _cache[ck]
    out = list(_plain_exits(key, variant, gated))
    a = navm.areas().get(key)
    if a is not None:
        m = a['maps'][min(max(variant, 1), len(a['maps'])) - 1]
        o = json.loads((navm.CONTENT / 'maps' / m / 'objects.json').read_text())
        for ar in o.get('areas', []):
            if ar['key'] != key:
                continue
            for t in ar['objects']:
                if t['type'] not in ('script_trigger', 'event_trigger'):
                    continue
                for w in t.get('warps') or []:
                    to = w.get('to')
                    if not to or to == key or to.startswith('0-'):
                        continue
                    # the pad takes the footprint's left edge inside its rect: aim 4 px right of the centre
                    out.append((to, dict(t['rect'], x=t['rect']['x'] + 4), 'up'))
    out = [x for x in out if (key, x[0]) not in BLOCK]
    _cache[ck] = out
    return out


navm.exits = exits
_plain_approach = navm.approach


def approach(rect, push):
    """nav.approach, with the footprint's top-left point kept inside a narrow exit's rect (an exit fires only while
    that point is inside it across the push direction: 12-12's 9 px door)"""
    cx, cy = _plain_approach(rect, push)
    if push in ('up', 'down'):
        cx = max(cx, rect['x'] + 3)
    elif push in ('left', 'right'):
        cy = max(cy, rect['y'] + 3)
    return cx, cy


navm.approach = approach


def map_item(e):
    """the first map item's box (x0, y0, x1, y1), or None"""
    n = e.r32(0x03001C4C + 4)
    for k in range(min(n, 255)):
        o = e.r32(0x03001C4C + 8 + 4 * k)
        if e.r32(o) == 0x0804C508:
            return [e.r32(o + i) for i in (0x18, 0x1C, 0x20, 0x24)]
    return None


class DbDriver(ShipDriver):
    AVOID = (20, 64)
    NEAR = 56
    HEALS = (7, 6, 8, 9, 21, 5, 4, 3, 2, 1, 0)   # Steak / Turkey first, the Senzu when they run out
    rush = False
    EMERGENCY = 0.2     # heal anywhere below this fraction of max HP
    TOPUP = 0.6         # heal before an exit below this

    def maybe_heal(self, e, s, frac=None):
        return super().maybe_heal(e, s, self.EMERGENCY if frac is None or frac == 0.35 else frac)

    def flame_guard(self, e, s):
        return None if self.rush else super().flame_guard(e, s)

    def block_guard(self, e, s):
        return None if self.rush else super().block_guard(e, s)

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        s = status(e)
        if kind == 'go':
            # ('go', w, a, mode): mode True = clear every room (ShipDriver's nav + fights); False = rush (no fights,
            # no flame / block guard: a Destroyer in a corridor is run through); 'near' = rush, but an enemy within
            # NEAR px of the player is fought first (a Mechanoid in the way knocks a runner about)
            mode = g[3] if len(g) > 3 else False
            self.rush = mode is not True
            try:
                if mode == 'near' and s['scene'] == 0 and s['p'] is not None:
                    x, y = s['p'][0], s['p'][1]
                    if any(o[5] not in self.AVOID and abs(o[1] - x) + abs(o[2] - y) <= self.NEAR
                           and o[0] not in self.skip for o in enemies(e)):
                        if self.maybe_heal(e, s):
                            return self.heal_step(e) or 0
                        self.in_nav = True
                        self.fight_done = False
                        k = self.fightp(e, s, ('fightp', 10 ** 9, self.NEAR, 6))
                        self.push_t = None
                        return k
                was = self.push_t
                k = self.nav(e, s, ('nav', g[1], g[2], mode is True))
                # about to push into an exit: top up there (the pause menu's timing is not modelled exactly yet, so a
                # heal costs a segment's remaining frames - at an exit that is the least)
                if self.push_t is not None and was is None and self.maybe_heal(e, s, self.TOPUP):
                    self.push_t = None
                    return self.heal_step(e) or 0
                return k
            finally:
                self.rush = False
        if kind == 'waitcd':
            # ('waitcd', n): stand still until the script countdown (GD+0x4D4) is <= n
            if e.r32(0x03001068 + 0x4D4) <= g[1]:
                self.next_goal()
                return None
            return 0
        if kind == 'clear':
            if s['scene'] > 0 or s['p'] is None:
                self.idle = 0
                return self.mash_a()
            if not enemies(e):
                self.idle += 1
                if self.idle >= 40:
                    self.next_goal()
                    return None
                return 0
            self.in_nav = True
            self.fight_done = False
            keep = self.idle
            k = self.fightp(e, s, ('fightp', g[1], 99999, 6, True))
            self.in_nav = False
            self.idle = 0 if enemies(e) else keep
            return k
        if kind == 'item':
            # ('item',): walk onto the area's map item (object vt 0x0804C508, box at +0x18) and play its event
            if s['scene'] > 0:
                self.goals[self.k] = ('events',)
                return self.mash_a()
            if s['p'] is None:
                return 0
            if self.t > 1500:
                raise RuntimeError(f'item not reached {g}')
            box = map_item(e)
            if box is None:
                return 0
            # the item is solid and taken with A while facing it: walk to its top edge, then DOWN + A
            x, y = s['p'][0], s['p'][1]
            if abs(x - box[0]) > 2 or abs(y - (box[1] + 4)) > 6:
                return self.walk_keys(e, s, box[0], box[1] + 4)
            ph = self.t % 24
            return KEY.DOWN if ph < 6 else KEY.A if 8 <= ph < 11 else 0
        if kind == 'trig':
            if s['scene'] > 0:
                self.goals[self.k] = ('events',)
                return self.mash_a()
            if s['p'] is None:
                return 0
            x, y = s['p'][0], s['p'][1]
            if abs(x - g[1]) > 2 or abs(y - g[2]) > 2:
                if self.t > 3000:
                    raise RuntimeError(f'trigger not reached {g}')
                return self.walk_keys(e, s, g[1], g[2])
            self.mash += 1
            return g[3] if self.mash % 12 < 6 else 0
        return super().step(e)
