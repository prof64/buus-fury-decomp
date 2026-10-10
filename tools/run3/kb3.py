"""Run 3's Kid Buu part (`k3_kidbuu`; engine/docs/segments.md "Run 3", re-split of 2026-10-06: Earth agent = Kid Buu ->
credits + post-game). Starts where the side-content agent's `b3_superbuu` stops: leaving Inside Buu 21-26 for 20-8
after the brain fight (run 2's row 1,853,852), its last in-game save at 21-25. Run 2's Kid Buu goals
(tools/superbuu/sbroute2.py goals_run2, from 'd03_kid_buu_saga') with run 3's stresses and choices on top, driven by
sb3.SbDriver3 (SbDriver2 + Run3Driver's stress goals) + the goals below. Ends like sb3's old tail: after Kid Buu a Save
at West City 4-1, then into Capsule Corp 4-20 (the post-game, tools/run3/post3.py, Continues from there). Input only.

Run-3 choices / stresses (COVERAGE below):
- Goku's and Vegeta's points split another way again (STR / END / SPD; run 1 STR / END halves, run 2 POW / SPD)
- the switch screen browsed at every checkpoint (RIGHT x4, LEFT x4 over the greyed-out party, back on the leader)
- a Kyodai Pyramid grind (14-1 <-> 14-2) for **another Dragon Ranking**: the ending's ranking reads the party's total
  XP in 5M steps (run 1 31.05M -> #1479, run 2 22.46M -> #1477); 'rankgrind' levels Goku in steps saved at 14-1 until
  the total (plus Kid Buu's XP) lands in a bracket neither run had (25M .. 30M -> #1478, or 35M .. 40M)
- a death right after a level-up (the popup's frames) at the Pyramid -> Continue at 14-1's save
- post-game: a death in a world-map encounter arena (searched for one with enemies), HP worn down first in 14-2
- the pause spun before every boss fight, START on alternate frames as every scripted stretch lets go and through
  every area change of a walk between areas (sb3's)

  ('rankgrind', areas)   grind steps (3 levels each, a checkpoint at 14-1 before each) until the party's total XP
                         + KB_XP is out of the brackets runs 1 / 2 ended in
  ('hpdown', frac|'hits', tag)  stand by the nearest enemy until HP <= frac of max (or two of the largest hit)
  ('subdrop', n, tag)    search a wait before flying onto the nearest encounter patrol for an arena with enemies,
                         then 'padhp' / 'padgo' / 'paddeath' (the exit pad's A on the killing hit's frame) / 'die1'
  ('padout',)            from 14-1 or the map: onto the world map

play: python3 tools/run3/kb3.py START.ss START.eeprom OUTDIR [--cap N] -> OUTDIR/keys.json + end.ss / end.eeprom
(one session, the cartridge save loaded first); --verify OUTDIR replays OUTDIR/keys.json and compares byte for byte.
Dev start (until b3 lands): work/earth/kb3/dev_row.ss + dev_row.eeprom (run 2 replayed to row 1,853,851)."""
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 're/buu/ship', 'tools/run2',
          'tools/run3'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
import sbemu  # noqa: E402  (the savedata-capable emulator lib first)
import sbroute2  # noqa: E402
from sbroute2 import KEY, ZMART_CURRY, level_of  # noqa: E402
from sb3 import SbDriver3, checkpoint3, stressed  # noqa: E402
from route import start_emu, status  # noqa: E402

A, B, L, R, U, D = KEY.A, KEY.B, KEY.LEFT, KEY.RIGHT, KEY.UP, KEY.DOWN
REC = 0x03001560          # character records, 0x68 each (+0x24 XP)
PYR = [(14, 1), (14, 2)]
KB_XP = 200_000           # the Kid Buu fight's XP (run 2: Goku and Vegeta 96,175 each)
# the ending's ranking brackets (5M steps of the party's total XP) runs 1 / 2 ended in
TAKEN = ((30_000_000, 35_000_000), (20_000_000, 25_000_000))
MARGIN = 400_000


def party_xp(e):
    return sum(e.r32(REC + 0x68 * c + 0x24) for c in range(5))


def rank_target(total):
    """the total XP to grind to (0: none) so that total lies outside TAKEN"""
    for lo, hi in TAKEN:
        if lo <= total < hi:
            return hi + MARGIN
    return 0


def checkpoint_k(name, x, y, w, a):
    """sb3's checkpoint3 with the switch screen browsed (RIGHT / LEFT over the party) instead of in-and-out"""
    g = checkpoint3(name, x, y, w, a)
    k = next(i for i, x_ in enumerate(g) if x_[0] == 'keys' and x_[1][:2] == [(A, 3, 40), (D, 3, 12)])
    ks = list(g[k][1])
    ks[3:3] = [(R, 3, 20)] * 4 + [(L, 3, 20)] * 4
    g[k] = ('keys', ks)
    return g


# the world-map encounter patrols: the Rogue Submarines (0-52, s509: 25 % an enemy group), the Stolen Airships (0-55)
PATROL_GFX = (0x086B43FC, 0x086A89D0)
LV_XP = None


def xp_to_next(e, c=0):
    global LV_XP
    if LV_XP is None:
        LV_XP = [x['total_xp'] for x in json.load(open(ROOT / 'data/buu/levels.json'))['levels']]
    xp = e.r32(REC + 0x68 * c + 0x24)
    return min(t for t in LV_XP if t > xp) - xp


def hp_of(e, c=0):
    return e.r16(REC + 0x68 * c + 0x16), e.r16(REC + 0x68 * c + 0x18)


ONCE = ('hpdown', 'subdrop', 'play', 'padhp', 'padgo', 'paddeath')


class KbDriver(SbDriver3):
    def step(self, e):
        g = self.goals[self.k]
        if g[0] in ONCE and g[-1] in self.died:
            self.next_goal()
            return None
        if g[0] == 'play':
            # ('play', keys, tag): a searched key sequence, one per frame (skipped once `tag` died)
            if self.t >= len(g[1]):
                self.next_goal()
                return None
            return g[1][self.t]
        if g[0] == 'hpdown':
            # ('hpdown', frac | 'hits', tag): stand by the nearest enemy (no punches, no food) until HP <= frac of max,
            # or ('hits') at most two of the largest hit seen so far
            s = status(e)
            hp, mx = hp_of(e)
            last = getattr(self, 'hd_last', None)
            if last is not None and hp < last:
                self.hd_max = max(getattr(self, 'hd_max', 0), last - hp)
            self.hd_last = hp
            done = (hp <= 2 * self.hd_max) if g[1] == 'hits' and getattr(self, 'hd_max', 0) else \
                (g[1] != 'hits' and hp * 100 <= mx * int(g[1] * 100))
            if done and s['scene'] == 0:
                print('hpdown:', hp, '/', mx, 'largest hit', getattr(self, 'hd_max', 0), flush=True)
                self.hd_last = None
                self.next_goal()
                return None
            self.heal_cool = 10 ** 6
            from ship_route2 import enemies
            if s['p'] is not None and s['scene'] == 0 and not [o for o in enemies(e) if o[4] > 0]:
                # (none left here: out and back in brings them back)
                self.hd_none = getattr(self, 'hd_none', 0) + 1
                if self.hd_none > 300:
                    self.hd_none = 0
                    w, a = s['area'][:2]
                    other = next(x for x in PYR if list(x) != [w, a])
                    self.goals[self.k:self.k] = [('nav', other[0], other[1], False), ('nav', w, a, False)]
                    self.t = 0
                    return None
            else:
                self.hd_none = 0
            return self.swapped(e, ('die',))
        if g[0] == 'padhp':
            return self.padhp(e, g)
        if g[0] == 'paddeath':
            return self.paddeath(e, g)
        if g[0] == 'padout':
            # ('padout',): on the world map: done; in 14-1: its pad up to the map first
            if status(e)['area'][:2] == [0, 50]:
                self.next_goal()
                return None
            self.goals[self.k:self.k] = [('unstick',), ('walk', 146, 214), ('talk', 'U')]
            self.t = 0
            return None
        if g[0] == 'padgo':
            # ('padgo', tag): onto the arena's exit pad
            s = status(e)
            if s['p'] is None or s['scene'] > 0:
                return 0
            px, py = self.pad_spot(s)
            if abs(s['p'][0] - px) <= 4 and abs(s['p'][1] - py) <= 4 or self.t > 600:
                self.next_goal()
                return None
            return self.walk_keys(e, s, px, py)
        if g[0] == 'subdrop':
            return self.subdrop(e, g)
        if g[0] == 'subfly':
            return self.subfly(e, status(e))
        if g[0] == 'standw':
            # ('standw', n): n frames by the nearest enemy (as 'die': no punches)
            if self.t >= g[1]:
                self.next_goal()
                return None
            return self.swapped(e, ('die',))
        if g[0] == 'sub52':
            # (a search's last goal: the search's check ends it)
            if self.t >= 20000:
                self.next_goal()
                return None
            return 0
        if g[0] == 'rankgrind':
            total = party_xp(e) + KB_XP
            tgt = rank_target(total)
            if not hasattr(self, 'rank_t0'):
                self.rank_t0 = tgt
                print('rankgrind: total', total, '-> target', tgt, flush=True)
            if tgt == 0 or total >= self.rank_t0:
                print('rankgrind: done at', total, 'Goku', level_of(e, 0), flush=True)
                self.next_goal()
                return None
            lv = level_of(e)
            step = [('nav', 14, 1, False), *checkpoint_k(f'k03_goku{lv}', 90, 240, 14, 1), ('autoheal', True),
                    ('grind', lv + 3, list(g[1])), ('retry_off',)]
            self.goals[self.k + 1:self.k + 1] = step + [g]
            self.next_goal()
            return None
        return super().step(e)

    def swapped(self, e, h):
        """run goal h in this goal's place for one frame (the goal itself stays)"""
        j = self.k
        g = self.goals[j]
        self.goals[j] = h
        try:
            r = super().step(e)
        finally:
            if self.k == j:
                self.goals[j] = g
        if self.k != j:
            # h ended by itself: stay on this goal
            self.k, self.t = j, 0
            return 0 if r is None else r
        return r

    def search(self, e, goals, ok, n, cap):
        """try sub-drivers KbDriver(goals(i)) from the current state, i = 0 .. n-1, each at most `cap` frames;
        ok(e, pre, post) on every frame (pre / post: (hp, level, scene)) says success. The first success's keys
        (or None)"""
        base = e.save()
        found = None
        for i in range(n):
            e.load(base)
            sub = KbDriver(goals(i))
            sub.heal_cool = 10 ** 9
            sub.autoheal = False
            ks = []
            pre = (hp_of(e)[0], level_of(e, 0), status(e)['scene'])
            for _ in range(cap):
                if sub.done():
                    break
                k = sub.keys(e) or 0
                e.set_keys(k)
                e.run(1)
                ks.append(k)
                post = (hp_of(e)[0], level_of(e, 0), status(e)['scene'])
                r = ok(e, pre, post)
                if r is not None:
                    break
                pre = post
            else:
                r = None
            print(f'  search try {i}: {len(ks)} frames ->', r, post, flush=True)
            if r:
                found = ks
                break
        e.load(base)
        return found

    def subdrop(self, e, g):
        """('subdrop', n, tag): on the world map, find a wait w (0 .. n-1, 7 frames apart) before flying onto the
        nearest patrol (a Rogue Submarine: 0-52, s509 an enemy group 25 %; a Stolen Airship: 0-55) whose encounter
        arena has enemies; play it, then 'padhp' / 'paddeath' there"""
        def goals(i):
            return [('wait', 1 + 7 * i), ('subfly',), ('sub52',)]

        def ok(e, pre, post):
            s = status(e)
            if s['area'][0] != 0 or s['area'][:2] == [0, 2]:
                return False
            if 51 <= s['area'][1] <= 59 and s['p'] is not None and s['scene'] == 0:
                from ship_route2 import enemies
                self.s52 = getattr(self, 's52', 0) + 1
                if self.s52 >= 40:
                    self.s52 = 0
                    return bool([o for o in enemies(e) if o[4] > 0])
            else:
                self.s52 = 0
            return None
        ks = self.search(e, goals, ok, g[1], 8000)
        if ks is None:
            # (no group in n tries: the leader stays on the map - 'padout' goes on from there)
            print('subdrop: no enemy group found - skipped', flush=True)
            self.died.add(g[-1])
            self.next_goal()
            return None
        print(f'subdrop: found ({len(ks)} frames)', flush=True)
        # (the tag counts as died from the die1 at the end on: its Continue skips all of these)
        tag = g[-1]
        self.goals[self.k:self.k + 1] = [('play', ks, tag), ('padhp', tag), ('padgo', tag), ('paddeath', 120, tag),
                                         ('die1', tag)]
        self.t = 0
        return None

    def pad_spot(self, s):
        """the current area's pad back to the world map (re/buu/ship/ship_driver.py map_pad - the r1frozen driver
        in this class's bases has none)"""
        import nav as navm
        key = f"{s['area'][0]}-{s['area'][1]}"
        o = json.loads((navm.CONTENT / 'maps' / navm.areas()[key]['maps'][0] / 'objects.json').read_text())
        for ar in o['areas']:
            for t in ar['objects']:
                if any(w.get('to') == '0-50' for w in t.get('warps') or []):
                    r = t['rect']
                    return r['x'] + r['w'] // 2 - 8, r['y'] + r['h'] - 10
        raise RuntimeError(f'no map pad in {key}')

    def padhp(self, e, g):
        """('padhp',): in an encounter arena, stand on its exit pad (no punches, no food) until HP is within two of
        the largest hit seen (or 6000 frames without a hit); after 400 quiet frames it fetches an enemy"""
        s = status(e)
        self.heal_cool = 10 ** 6
        if s['p'] is None or s['scene'] > 0:
            return 0
        hp = s['hp']
        last = getattr(self, 'ph_last', None)
        if last is not None and hp < last:
            self.ph_max = max(getattr(self, 'ph_max', 0), last - hp)
            self.ph_quiet = 0
        self.ph_last = hp
        self.ph_quiet = getattr(self, 'ph_quiet', 0) + 1
        m = getattr(self, 'ph_max', 0)
        if (m and hp <= 2 * m) or self.ph_quiet > 6000:
            print('padhp:', hp, 'largest hit', m, flush=True)
            self.ph_last = None
            self.next_goal()
            return None
        # (the arena's enemies keep to themselves: after 400 quiet frames on the pad, fetch one - walk up to the
        # nearest until it hits, then back onto the pad with it following)
        if self.ph_quiet > 400 and not getattr(self, 'ph_lure', False):
            self.ph_lure = True
        if getattr(self, 'ph_lure', False):
            if self.ph_quiet <= 1:
                self.ph_lure = False
            else:
                return self.swapped(e, ('die',))
        px, py = self.pad_spot(s)
        if abs(s['p'][0] - px) > 4 or abs(s['p'][1] - py) > 4:
            return self.walk_keys(e, s, px, py)
        return 0

    def paddeath(self, e, g):
        """('paddeath', n): standing on the arena's exit pad, A after w = 0 .. n-1 frames: the first try where the
        pad's script starts on the frame an enemy's hit takes the last HP is played (the soak agent's 'death by the
        hit on a script's first frame'); none: stand until dead ('die' follows)"""
        def goals(i):
            return [('play', [0] * i + [A] + [0] * 90, None)]

        def ok(e, pre, post):
            if post[0] == 0 and pre[0] > 0:
                return pre[2] == 0 and post[2] > 0
            if post[2] > 0 and pre[2] == 0:
                return False
            return None
        ks = self.search(e, goals, ok, g[1], g[1] + 100)
        if ks is None:
            print('paddeath: no same-frame death found - a plain death', flush=True)
            self.next_goal()
            return None
        print(f'paddeath: found ({len(ks)} frames)', flush=True)
        self.goals[self.k] = ('play', ks, None)
        self.t = 0
        return None

    PADS = None

    def over_pad(self, x, y):
        """near one of the world map's landing rects (re/buu/ship/ship_driver.py over_pad)"""
        import nav as navm
        if KbDriver.PADS is None:
            o = json.loads((navm.CONTENT / 'maps' / 'area_00_050' / 'objects.json').read_text())
            KbDriver.PADS = [t['rect'] for ar in o['areas'] for t in ar['objects'] if t.get('warps')]
        return any(r['x'] - 48 <= x < r['x'] + r['w'] + 48 and r['y'] - 48 <= y < r['y'] + r['h'] + 48
                   for r in KbDriver.PADS)

    def subfly(self, e, s):
        """fly low onto the nearest encounter patrol (ShipDriver.hunt's approach; Rogue Submarines or Stolen
        Airships); ends in the encounter arena (0-51 .. 0-59)"""
        import math
        from ship_route2 import enemies
        if s['area'][0] == 0 and 51 <= s['area'][1] <= 59:
            self.next_goal()
            return None
        if s['scene'] > 0 or s['p'] is None or s['area'][:2] != [0, 50]:
            return self.mash_a() if s['scene'] > 0 else 0
        p = e.r32(0x03001C34)
        if not p:
            return 0
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
        x, y = sv(e.r32(p + 0x1A0)) / 256, sv(e.r32(p + 0x1A4)) / 256
        z, h = sv(e.r32(p + 0x1A8)) / 1024, sv(e.r32(p + 0x1AC)) / 1024
        pats = [o for o in enemies(e) if o[5] == 141 and e.r32(o[0] + 0x58) in PATROL_GFX
                and not self.over_pad(o[1], o[2])]
        tx, ty = (1536, 1456) if not pats else min(((o[1], o[2]) for o in pats),
                                                   key=lambda q: math.hypot(q[0] - x, q[1] - y))
        dx, dy = tx - x, ty - y
        wantd = math.degrees(math.atan2(dx, -dy)) * 256 / 360
        diff = (wantd - h + 128) % 256 - 128
        k = 0
        if diff > 3:
            k |= R
        elif diff < -3:
            k |= L
        if abs(diff) < 40:
            k |= U
        dist = math.hypot(dx, dy)
        if dist > 90 or not pats:
            k |= B if z < 100 else 0
        elif z > 2 and not self.over_pad(x, y):
            k |= A
        return k


def goals_k3():
    sbroute2.checkpoint = checkpoint_k
    g = [
        ('mark', 'k00_start'),
        # out of Buu (s478 / s479) -> the world map
        ('eventsw', 240), ('wait', 30), ('retry_off',), ('autoheal', False), ('mark', 'k01_kid_buu_saga'),
        # West City 4-1: two Curry Plates; Goku's points STR / END / SPD (a third each, the rest SPD); Vegeta's from
        # Goku's Status page (his tab: RIGHT four times) STR / END halves; Vegeta rests full in reserve
        ('fly', 1250, 1200), ('until', 4, 1), ('areaw', 4, 1), ('wait', 30),
        *ZMART_CURRY,
        ('events',), ('allocnow', lambda n: [n // 3, 0, n // 3, n - 2 * (n // 3)]), ('wait', 30),
        ('allocfor', 4, 4, lambda n: [n // 2, 0, n - n // 2, 0]), ('closemenu',),
        ('wait', 30), ('rest', 4),
        *checkpoint_k('k02_4-1', 148, 594, 4, 1),
        # the pad -> Kyodai Pyramid 14-1 (its gate open since Gohan): a save there, one level and a death right after
        # it (-> Continue at 14-1), then the ranking grind in saved steps
        ('unstick',), ('walk', 410, 356), ('talk', 'U'), ('fly', 2177, 1201, 31, 23), ('until', 14, 1), ('wait', 30),
        *checkpoint_k('k03_14-1', 90, 240, 14, 1), ('autoheal', True),
        ('mark', 'k03_pyramid'),
        ('rankgrind', PYR),
        ('nav', 14, 1, False), *checkpoint_k('k04_14-1', 90, 240, 14, 1), ('autoheal', False),
        # back to West City: two more Curry Plates, the checkpoint at 4-1, the crater spot -> Kid Buu (19-3)
        ('unstick',), ('walk', 146, 214), ('talk', 'U'), ('fly', 1250, 1200), ('until', 4, 1), ('areaw', 4, 1),
        ('wait', 30), *ZMART_CURRY,
        *checkpoint_k('k05_4-1', 148, 594, 4, 1), ('autoheal', True),
        ('unstick',), ('walk', 410, 356), ('talk', 'U'), ('fly', 648, 1076), ('eventsw', 240), ('wait', 30),
        ('mark', 'k06_kid_buu'), ('luck',),
        ('boss', 60000, 'timer'), ('eventsw', 240), ('evade', 60000), ('evade', 60000), ('eventsw', 240), ('wait', 30),
        ('retry_off',), ('autoheal', False), ('mark', 'k07_after_kid_buu'),
        # --- post-game extras (engine/docs/segments.md, the soak agent's "Earth extras"): the Kyodai Pyramid from the
        # map, a save at 14-1; in 14-2 HP down to 25 % by standing among the enemies; out by the pad onto the nearest
        # world-map encounter patrol whose arena has enemies (searched); on the arena's exit pad HP down to two hits,
        # then the pad's A on the frame an enemy's hit kills (searched: the hit on a script's first frame) -> game
        # over -> Continue at 14-1. (A death on a level-up popup's first frame is impossible: the level-up refills HP
        # in that frame - 240 tries in 14-2, every one alive at full HP)
        ('fly', 2177, 1201, 31, 23), ('until', 14, 1), ('wait', 30), ('mark', 'k08_extras'),
        ('unstick',), ('walk', 90, 240),     # (off the landing pad: a walk across it takes off again)
        *checkpoint_k('k09_14-1', 90, 240, 14, 1), ('autoheal', False),
        ('nav', 14, 2, False), ('hpdown', 0.25, 'mapdeath'), ('nav', 14, 1, False),
        ('unstick',), ('walk', 146, 214), ('talk', 'U'), ('subdrop', 120, 'mapdeath'),
        ('retry_off',), ('mark', 'k10_extras_done'),
        # the end: (in 14-1 after the Continue, or on the map) the pad -> West City's save point, then into Capsule
        # Corp 4-20 (the post-game part starts there)
        ('padout',), ('fly', 1137, 1137, 71, 119), ('until', 4, 1), ('areaw', 4, 1),
        ('wait', 30), ('unstick',), ('walk', 148, 594), ('menutour',),
        ('unstick',), ('walk', 148, 594), ('face', 'U'), sbroute2.SAVE, ('bout',),
        ('mark', 'k11_saved_4-1'), ('exitto', '4-20'), ('wait', 60), ('mark', 'k12_end'),
    ]
    out = stressed(g)
    # a death right after a level-up: after 14-1's checkpoint (its luck goal)
    k = out.index(('mark', 'k03_pyramid'))
    out[k + 1:k + 1] = [('levelup1', PYR), ('die1', 'k03_14-1')]
    return out


COVERAGE = """
Kid Buu (k3_kidbuu): out of Buu -> West City (two Curry Plates, Goku's and Vegeta's points STR / END / SPD - a third
split), Vegeta rested; the Kyodai Pyramid with Goku: a save, one level then a planned game over -> Continue at 14-1,
a grind in saved steps until the party's XP lands in a Dragon Ranking bracket runs 1 / 2 did not reach (25M .. 30M:
#1478); two more curries; Kid Buu (19-3) with food and retries from 4-1's save. Post-game extras: the Pyramid again,
a save at 14-1, HP worn down among 14-2's enemies, the world map's nearest encounter patrol whose arena has enemies
(searched), there HP down to two hits on the arena's exit pad, the pad's A searched for the frame of the killing hit
(none found: a plain death) -> game over -> Continue at 14-1; a Save at 4-1, into Capsule Corp 4-20. Stresses: the
switch screen browsed (RIGHT x4 / LEFT x4) at every checkpoint, the pause pages after every Continue, the pause spun
before every boss fight, START on alternate frames as every scripted stretch lets go and through every walked area
change.
"""


def run(st, ee, out, cap, frm=None):
    e = start_emu(str(Path(st).resolve()))
    sbemu.sd_write(e, Path(ee).read_bytes())
    goals = goals_k3()
    if frm:
        # (development: from a mark on)
        goals = goals[goals.index(('mark', frm)):]
    d = KbDriver(goals)
    log = []
    prev = None
    t0 = time.time()
    try:
        while not d.done():
            if len(log) >= cap:
                raise RuntimeError(f'cap at goal {d.k} {d.goals[d.k]}')
            k = d.keys(e) or 0
            e.set_keys(k)
            e.run(1)
            log.append(k)
            if len(log) % 20 == 0 or d.k != (prev[1] if prev else None):
                s = status(e)
                if prev is None or (s['area'][:2], d.k) != prev:
                    prev = (s['area'][:2], d.k)
                    print(' ', len(log), d.k, str(d.goals[d.k] if d.k < len(d.goals) else '')[:80], s, flush=True)
    except Exception as ex:  # noqa: BLE001 - keep what ran, show where
        e.save_file(str(out / 'fail.ss'))
        (out / 'fail.eeprom').write_bytes(sbemu.sd_read(e))
        e.screenshot(str(out / 'fail.png'), 2)
        (out / 'keys.json').write_text(json.dumps({'keys': log, 'failed': True}))
        print(f'FAIL after {len(log)} frames at goal {d.k} {d.goals[d.k] if d.k < len(d.goals) else ""}: {ex}',
              status(e), flush=True)
        sys.exit(1)
    (out / 'keys.json').write_text(json.dumps({'keys': log}))
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    (out / 'end.eeprom').write_bytes(sbemu.sd_read(e))
    print(f'k3_kidbuu: {len(log)} frames {time.time() - t0:.0f}s', status(e), 'party XP', party_xp(e),
          'planned deaths', sorted(d.died), flush=True)


def verify(st, ee, out):
    e = start_emu(str(Path(st).resolve()))
    sbemu.sd_write(e, Path(ee).read_bytes())
    for k in json.loads((out / 'keys.json').read_text())['keys']:
        e.set_keys(k)
        e.run(1)
    e.set_keys(0)
    ref = start_emu(str(out / 'end.ss'))
    same = (e.ewram() == ref.ewram() and e.iwram() == ref.iwram()
            and sbemu.sd_read(e) == (out / 'end.eeprom').read_bytes())
    print('verify:', 'byte-identical' if same else 'DIFFERS', status(e))
    return same


def main():
    a = sys.argv[1:]
    st, ee, out = a[0], a[1], Path(a[2])
    out.mkdir(parents=True, exist_ok=True)
    if '--verify' in a:
        sys.exit(0 if verify(st, ee, out) else 1)
    run(st, ee, out, int(a[a.index('--cap') + 1]) if '--cap' in a else 1500000,
        a[a.index('--from') + 1] if '--from' in a else None)


if __name__ == '__main__':
    main()
