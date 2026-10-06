"""tours.py -- field coverage tours (headless, muted): from an emulator save state, drive the player through a goal
list and record the game as a segment (segment.py's format) for the segment harness (wf-import tests/segments.rs).

  tours.py goals AREA [VARIANT]           print the interaction goals derived from the area's objects
  tours.py record STATE NAME GOALS [--frames N]
        GOALS: a Python goal list (the route driver's goals, tools/story_playthrough/driver.py, plus the ones below),
        or 'area:W-A[:V]' for the derived goals of that area (the player must already be there)
  env SEGMENT_DIR (default work/segs_tours)

Extra goals:
  ('talk_npc', k)       walk up to the k-th NPC actor (kinds npc / npc_talk, creation order = the objects.json NPC order), face it, A, play the event
                        through (gives up after 600 frames)
  ('a_at', x, y, dir)   walk to (x, y) (footprint top-left px), face dir, A, play the event through
Derived goals (an area's objects.json): every A-activated trigger (event / script / save point / computer), every
talkable NPC, every map item (walked onto). Breakables and enemies are left out (fights are their own segments).
"""
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
from sbemu import sd_read, sd_write  # noqa: E402  (the savedata-capable emulator lib first: tours carry the EEPROM)
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
import segment as sg  # noqa: E402
from driver import Driver, DIRS  # noqa: E402
from route import KEY, start_emu, status  # noqa: E402
from common_field import objs  # noqa: E402

CONTENT = ROOT / 'work' / 'byor' / 'content' / 'base'
FACING = {'up': 'U', 'down': 'D', 'left': 'L', 'right': 'R', '': 'U'}


def area_goals(key, variant=None):
    w, a = map(int, key.split('-'))
    d = json.loads((CONTENT / 'maps' / f'area_{w:02d}_{a:03d}' / 'objects.json').read_text())
    goals = []
    for ar in d.get('areas', []):
        if variant is not None and ar['variant'] != variant:
            continue
        npc_k = 0
        for o in ar['objects']:
            t = o['type']
            act = o.get('activation') or {}
            if t in ('event_trigger', 'script_trigger', 'save_point', 'trigger_unknown') and act.get('mode') == 'button':
                r = o['rect']
                f = act.get('facing', '')
                # stand inside the rect, at its edge on the side the player faces from
                x = r['x'] + r['w'] // 2 - 8
                y = {'up': r['y'] + r['h'] - 8, 'down': r['y'], 'left': r['y'] + r['h'] // 2 - 4,
                     'right': r['y'] + r['h'] // 2 - 4}.get(f, r['y'] + r['h'] // 2 - 4)
                if f == 'left':
                    x = r['x'] + r['w'] - 16
                if f == 'right':
                    x = r['x']
                goals.append(('a_at', max(0, x), max(0, y), FACING[f]))
            elif t == 'npc' and o.get('talk'):
                goals.append(('talk_npc', npc_k))
            elif t == 'item':
                p = o.get('pos') or [o.get('x', 0), o.get('y', 0)]
                goals.append(('walk', int(p[0]) - 8, int(p[1]) - 4))
            if t == 'npc':
                npc_k += 1
        break
    return goals


class TourDriver(Driver):
    def mash_a(self):
        # A every 30 frames; in choice boxes the cursor walks down one more option each round (0, 1, 2, 3, then the
        # last ones again), so menus are explored and their last option (usually the way out) is reached
        self.mash += 1
        c, r = self.mash % 30, (self.mash // 30) % 6
        downs = min(r, 4)
        if c < 4:
            return KEY.A
        if 8 <= c < 8 + 4 * downs and (c - 8) % 4 < 2:
            return KEY.DOWN
        return 0

    def step(self, e):
        g = self.goals[self.k]
        # a goal that never ends (an event loop, a blocked walk) is given up after 4000 frames
        if self.t > 4000 and g[0] not in ('wait', 'hold'):
            self.next_goal()
            return 0
        if g[0] == 'a_at':
            s = status(e)
            if self.t == 0:
                self.sub, self.st = 'walk', 0
            self.st += 1
            if self.sub == 'walk':
                if s['scene'] > 0:
                    return self.mash_a()
                if s['p'] is not None and (abs(s['p'][0] - g[1]) <= 1 and abs(s['p'][1] - g[2]) <= 1
                                           or self.stuck > 60 or self.st > 900):
                    self.sub, self.st = 'press', 0
                    return 0
                return self.walk_point(e, s, g[1], g[2])
            if self.st < 3:
                return DIRS[g[3]]
            if self.st < 5:
                return 0
            if self.st < 8:
                return KEY.A
            if s['scene'] > 0:
                self.idle = 0
                return self.mash_a()
            self.idle += 1
            if self.idle >= 30:
                self.next_goal()
                return None
            return 0
        if g[0] == 'talk_npc':
            s = status(e)
            if self.t == 0:
                self.sub = 'go'
            if self.sub == 'go':
                if s['scene'] > 0:
                    self.sub = 'event'
                    return self.mash_a()
                npcs = [o for o in objs(e) if o[0] in ('npc', 'npc_talk') and o[2] is not None]
                if g[1] >= len(npcs) or self.t > 600 or s['p'] is None:
                    self.next_goal()
                    return None
                nx, ny = npcs[g[1]][2], npcs[g[1]][3]
                x, y = s['p'][0], s['p'][1]
                if abs(x - nx) <= 6 and 6 <= y - ny <= 14:
                    # below it: face up and A (every few frames until an event starts)
                    return KEY.UP if self.t % 8 < 2 else (KEY.A if self.t % 8 < 4 else 0)
                return self.walk_point(e, s, nx, ny + 10)
            if s['scene'] > 0:
                self.idle = 0
                return self.mash_a()
            self.idle += 1
            if self.idle >= 30:
                self.next_goal()
                return None
            return 0
        return super().step(e)

    def walk_point(self, e, s, tx, ty):
        """one step of walk() toward (tx, ty) without finishing the goal"""
        k0, t0, st = self.k, self.t, self.stuck
        k = self.walk(e, s, tx, ty)
        if self.k != k0:
            self.k, self.t, self.stuck = k0, t0, st + 1
            self.path = None
            return 0
        return k or 0


def record(state, name, goals, frames=20000):
    p = Path(state)
    if not p.is_absolute() and not p.exists():
        p = ROOT / 'work' / 'states' / state
    e = start_emu(str(p))
    # the cartridge's EEPROM (the in-game saves) next to the state (tools/run2/state_at.py writes it): a save state
    # does not carry it; the segment does (init.eeprom, as the whole-run cuts) - the Save screen shows the slots
    ep = Path(str(p) + '.eeprom')
    if ep.exists():
        sd_write(e, ep.read_bytes())
    drv = TourDriver(goals)
    init, ob, rows = sg.record(e, lambda j, ee: None if drv.done() else drv.keys(ee), frames)
    if ep.exists():
        import base64
        import zlib
        init['eeprom'] = base64.b64encode(zlib.compress(ep.read_bytes())).decode()
    doc = {'kind': 'segment', 'name': name, 'source': {'kind': 'state', 'state': state, 'goals': repr(goals)},
           'init': init, 'objs': ob, 'frames': rows}
    os.environ.setdefault('SEGMENT_DIR', str(ROOT / 'work' / 'segs_tours'))
    p = sg.save(name, doc)
    print(p, len(rows), 'frames; start', init['area'], 'end', rows[-1]['area'] if rows else None, 'goals done', drv.k, '/', len(goals))


if __name__ == '__main__':
    a = sys.argv[1:]
    if a[0] == 'goals':
        print(area_goals(a[1], int(a[2]) if len(a) > 2 else None))
    elif a[0] == 'record':
        g = a[3]
        if g.startswith('area:'):
            p = g[5:].split(':')
            goals = area_goals(p[0], int(p[1]) if len(p) > 1 else None)
        else:
            import ast
            goals = ast.literal_eval(g)
        fr = int(a[a.index('--frames') + 1]) if '--frames' in a else 20000
        record(a[1], a[2], goals, fr)
