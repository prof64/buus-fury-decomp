"""field coverage tours for world 1 (Other World), 5 (Roshi's Island), 6 (Kami's Lookout / Palace / Hyperbolic Time
Chamber), 7 (Papaya Island, World Tournament Stadium). States work/states/tour_kami_*.ss (tools/run2/state_at.py):
  snakeway0   run1 row     6,800  1-1 Snakeway (game start: the three souls by the start, s041..s043)
  snakeway    run1 row   995,400  1-1 Snakeway (late game, Goku back in Other World: the s044..s047 souls)
  yemma       run1 row 1,198,010  1-3 Yemma's Office
  yemma4      run1 row    10,090  1-4 Yemma's Office back room (game start; late game an ogre blocks the door)
  grandkai    run2 row    51,970  1-10 Grand Kai's Mansion (after the Cave of the Ancients)
  cave        run3 row    30,190  1-16 Cave of the Ancients
  lookout     run1 row   958,300  6-1 Kami's Lookout
  lookout20   run1 row 1,232,380  6-20 Kami's Lookout (Kid Buu arc)
  htc         run1 row 1,216,600  6-9 Hyperbolic Time Chamber
  papaya      run1 row   148,500  7-2 Papaya Island
  wm          run1 row   203,780  0-50 world map, flying off Papaya Island (-> Roshi's Island)"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tours  # noqa: E402
from tours import area_goals, TourDriver, objs, status, KEY  # noqa: E402

# ('talk_near', x, y): talk to the NPC nearest (x, y) when the goal starts. tours.py's 'talk_npc' only looks at the
# 'npc_talk' kind (vtable 0x0804CEF4, ctor 0x080111A9 = the NON-talking NPCs: shopkeepers, statues); the talkable NPCs
# (objects.json 'talk', ctor 0x08017DA3) are the 'npc' kind (0x0804C92C). Approach from below / left / right / above
# in turn (wandering NPCs are followed), face it, A, play the event through. Added as a new goal kind only (the
# existing goals are untouched).
SIDES = [((0, 10), KEY.UP), ((-18, 0), KEY.RIGHT), ((18, 0), KEY.LEFT), ((0, -10), KEY.DOWN)]
_step0 = TourDriver.step


def _step(self, e):
    g = self.goals[self.k]
    if g[0] != 'talk_near':
        return _step0(self, e)
    s = status(e)
    if self.t == 0:
        self.sub, self.tn_i, self.tn_side, self.tn_t = 'go', None, 0, 0
        if getattr(self, 'tn_area', None) != s['area']:
            self.tn_area, self.tn_used = s['area'], set()
        cand = [(abs(o[2] - g[1]) + abs(o[3] - g[2]), i) for i, o in enumerate(objs(e))
                if o[0] == 'npc' and o[2] is not None and i not in self.tn_used]
        if cand:
            self.tn_i = min(cand)[1]
            self.tn_used.add(self.tn_i)
    if self.sub == 'go':
        if s['scene'] > 0:
            self.sub = 'event'
            return self.mash_a()
        ob = objs(e)
        if self.tn_i is None or self.tn_i >= len(ob) or ob[self.tn_i][2] is None or self.t > 1200 or s['p'] is None:
            self.next_goal()
            return None
        nx, ny = ob[self.tn_i][2], ob[self.tn_i][3]
        self.tn_t += 1
        if self.tn_t > 300 or self.stuck > 50:
            self.tn_side, self.tn_t, self.stuck, self.path = (self.tn_side + 1) % 4, 0, 0, None
        (dx, dy), face = SIDES[self.tn_side]
        x, y = s['p'][0], s['p'][1]
        if abs(x - nx - dx) <= 4 and abs(y - ny - dy) <= 4:
            return face if self.t % 8 < 2 else (KEY.A if self.t % 8 < 4 else 0)
        return self.walk_point(e, s, nx + dx, ny + dy)
    if s['scene'] > 0:
        self.idle = 0
        return self.mash_a()
    self.idle += 1
    if self.idle >= 30:
        self.next_goal()
        return None
    return 0


TourDriver.step = _step


def npc_goals(key, variant=1):
    """'talk_near' goals for every talkable NPC of the area's objects.json (its spawn position)"""
    import json
    w, a = map(int, key.split('-'))
    d = json.loads((tours.CONTENT / 'maps' / f'area_{w:02d}_{a:03d}' / 'objects.json').read_text())
    out = []
    for ar in d.get('areas', []):
        if ar['variant'] != variant:
            continue
        for o in ar['objects']:
            if o['type'] == 'npc' and o.get('talk'):
                out.append(('talk_near', int(o['pos'][0]), int(o['pos'][1])))
        break
    return out


def trig_goals(key, variant=1):
    """area_goals without its (broken) talk_npc goals, plus talk_near goals"""
    return [g for g in area_goals(key, variant) if g[0] != 'talk_npc'] + npc_goals(key, variant)


def ex(path, w, a):
    """walk the points, then hold the last direction until the area is (w, a)"""
    return [('walk', x, y) for x, y in path] + [('area', w, a), ('wait', 40)]


def last(goals, i=0):
    """move goal i (an A-trigger that leaves the area) to the end"""
    g = list(goals)
    return g[:i] + g[i + 1:] + [g[i]]


def fly(name):
    """the world-map flight (work/shipfix/kami_fly.py output: [[keys, n], ...]) as 'hold' goals"""
    import json
    p = tours.ROOT / 'work' / 'states' / name
    return [('hold', k, n) for k, n in json.loads(p.read_text())]


SNAKEWAY0 = npc_goals('1-1')
SNAKEWAY = npc_goals('1-1')
YEMMA = trig_goals('1-3')
YEMMA4 = [('wait', 40)] + trig_goals('1-4')
GRANDKAI = (trig_goals('1-10') + ex([(264, 380), (264, 404)], 1, 11) + [('walk', 168, 48)] + trig_goals('1-11'))
# the boulder in front of the 1-17 door: L twice (ki technique), hold B 120 frames (charged blast), as run 3 does
CAVE = ([('wait', 40)] + trig_goals('1-16')
        + [('walk', 112, 138), ('hold', KEY.UP, 3), ('wait', 5), ('hold', 512, 2), ('wait', 12), ('hold', 512, 2),
           ('wait', 22), ('hold', 2, 120), ('wait', 60)]
        + ex([(112, 120)], 1, 17) + trig_goals('1-17'))
# 6-1: s213 (A at the palace steps), the save point, the NPCs, then the A-trigger to 6-13 (inline_0x08079030; its
# twin at (48, 312) and the capsule (304, 556) were used by the runs)
_L = area_goals('6-1', 1)
LOOKOUT = [_L[3], _L[4]] + npc_goals('6-1') + [_L[2], ('wait', 300)]
PALACE = (ex([(304, 270), (304, 240)], 6, 2)
          + ex([(224, 180), (224, 156)], 6, 3)
          + ex([(104, 40), (104, 12)], 6, 15)
          + ex([(104, 200), (104, 220)], 6, 3)
          + ex([(104, 200), (104, 220)], 6, 2)
          + ex([(100, 100), (100, 76)], 6, 14) + area_goals('6-14', 1)
          + ex([(112, 200), (112, 220)], 6, 2)
          + ex([(340, 90), (352, 76)], 6, 4)
          + ex([(224, 180), (224, 156)], 6, 5) + area_goals('6-5', 1))
LOOKOUT20 = last(area_goals('6-20', 1))
HTC = (ex([(190, 50), (190, 36)], 6, 6)
       + ex([(180, 156), (212, 156)], 6, 7)
       + ex([(30, 104), (2, 104)], 6, 6)
       + ex([(30, 156), (2, 156)], 6, 8)
       + ex([(190, 104), (212, 104)], 6, 6)
       + ex([(112, 200), (112, 226)], 6, 9)
       + ex([(30, 200), (2, 200)], 6, 10)
       + ex([(200, 200), (212, 200)], 6, 9)
       + ex([(360, 200), (372, 200)], 6, 11)
       + ex([(20, 200), (2, 200)], 6, 9)
       + ex([(200, 360), (200, 384)], 6, 12))
PAPAYA2 = trig_goals('7-2')
# fly off Papaya Island (run 1's capsule take-off) to Roshi's Island (landing spot 2873..2904 x 1673..1696, z < 16),
# then the island's save point and NPCs, Kame House (5-2) and its upstairs (5-3), back out, the capsule last
ROSHI = (fly('tour_kami_wm_roshi.json') + [('area', 5, 1), ('wait', 60)] + last(trig_goals('5-1'))[:-1]
         + ex([(392, 350), (392, 326)], 5, 2) + ex([(184, 100), (184, 86)], 5, 3)
         + ex([(152, 110), (152, 86)], 5, 2) + ex([(184, 240), (184, 266)], 5, 1)
         + [last(trig_goals('5-1'))[-1], ('wait', 200)])

TOURS = {
    'kami_snakeway0': ('tour_kami_snakeway0.ss', SNAKEWAY0),
    'kami_snakeway': ('tour_kami_snakeway.ss', SNAKEWAY),
    'kami_yemma': ('tour_kami_yemma.ss', YEMMA),
    'kami_yemma4': ('tour_kami_yemma4.ss', YEMMA4),
    'kami_grandkai': ('tour_kami_grandkai.ss', GRANDKAI),
    'kami_cave': ('tour_kami_cave.ss', CAVE),
    'kami_lookout': ('tour_kami_lookout.ss', LOOKOUT),
    'kami_palace': ('tour_kami_lookout.ss', PALACE),
    'kami_lookout20': ('tour_kami_lookout20.ss', LOOKOUT20),
    'kami_htc': ('tour_kami_htc.ss', HTC),
    'kami_papaya2': ('tour_kami_papaya.ss', PAPAYA2),
    'kami_roshi': ('tour_kami_wm.ss', ROSHI),
    # continuations past a divergence (states saved by work/shipfix/kami_save.py at the goal's start)
    'kami_grandkai11': ('tour_kami_grandkai11.ss', GRANDKAI[9:]),
    'kami_lookout_b': ('tour_kami_lookout_b.ss', LOOKOUT[4:]),
}

