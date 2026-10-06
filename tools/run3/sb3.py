"""Run 3's Super Buu + Kid Buu part (`b3_superbuu`; engine/docs/segments.md "Run 3", claim of 2026-10-06): run 2's
goals (tools/superbuu/sbroute2.py goals_run2: HFIL 18-13 -> the Lookout / fusion -> Ultimate Gohan's grinds -> the
crater -> the wasteland -> Goku 20-5 / 20-6 -> 20-7 -> inside Buu -> Kid Buu) with run 3's stresses on top, driven by
SbDriver3 = SbDriver2 + Run3Driver's stress goals. It ends where the post-game begins: after Kid Buu, a Save at West
City 4-1's save point, then into Capsule Corp 4-20 - the 4-21 lobby's final-sequence trigger is NOT entered (the
post-game part Continues this save). Input only.

Stresses (COVERAGE below):
- every checkpoint (save point + Save + the game-over retry stretch) opens the Switch Characters screen and takes
  the current character again before saving (the switch screen in and out; only Goku / Vegeta / Gohan are ever
  selectable in this part, two of them dead or greyed out most of it), and after every Continue the pause pages both
  ways ('menutour', once per stretch)
- the pause spun three times before every boss / beam / Buu fight ('pausespin')
- START on alternate frames for 60 frames as every scripted stretch lets go ('events' / 'eventsw' ends: the fusion,
  Mystic Gohan, the brain fight, Kid Buu), and on alternate frames through the first 40 frames of every area change a
  'nav' / 'exitto' makes (the exit's load / fade-in)
- planned game overs -> Continue at three save states: the Pyramid grind right after a level-up (the popup's
  frames), Goku's 20-6 grind, inside Buu at 21-1 ('die1': walk into the nearest enemy and stand)
- the save point's first A closed-loop ('opensave', as fat3.py: a walk a pixel off the spot meets nothing)

play: python3 tools/run3/sb3.py START.ss START.eeprom OUTDIR [--cap N] -> OUTDIR/keys.json + end.ss / end.eeprom
(one session with the cartridge save loaded first); --verify OUTDIR replays OUTDIR/keys.json from the same pair and
compares the end state byte for byte."""
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 'tools/run2', 'tools/run3',
          're/buu/ship'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
import sbemu  # noqa: E402  (the savedata-capable emulator lib first)
import sbroute2  # noqa: E402
from sbroute2 import SbDriver2, KEY, SAVE, charsel, level_of  # noqa: E402
from run3_driver import Run3Driver  # noqa: E402
from route import start_emu, status  # noqa: E402

S, A, B = KEY.START, KEY.A, KEY.B
RUN3_KINDS = ('mashkeys', 'pausespin', 'menutour', 'textmash')
FIGHTS = ('boss', 'beam', 'itbuu')


def checkpoint3(name, x, y, w, a):
    """sbroute2.checkpoint with the switch screen in and out before the Save and the pause pages after the Continue
    (the retry index counted for the extra goal: 'retry_here3' 5 = the deathcont)"""
    return [('unstick',), ('walk', x, y), *charsel(0), ('unstick',), ('walk', x, y), ('face', 'U'), SAVE, ('bout',),
            ('jumpto', 5), ('deathcont',), ('until', w, a), ('wait', 30), ('menutour',),
            ('mark', name), ('retry_here3', 5), ('luck',)]


class SbDriver3(SbDriver2):
    """SbDriver2 with Run3Driver's stress goals (its handlers called as plain functions), the closed-loop save point
    A, the one-shot planned death and the START overlay through area changes in 'nav' / 'exitto'"""
    idle = 0

    def __init__(self, goals):
        out = []
        for g in goals:
            if (isinstance(g, tuple) and g and g[0] == 'keys' and out and out[-1] == ('face', 'U') and g[1]
                    and g[1][0][0] == KEY.A):
                out += [('opensave', g[1][0][2]), ('keys', list(g[1][1:]))]
                continue
            out.append(g)
        super().__init__(out)
        self.died = set()
        self.tm_area = None
        self.tm = 0

    def keys(self, e):
        k = super().keys(e)
        if self.done():
            return k
        kind = self.goals[self.k][0]
        if kind in ('nav', 'exitto', 'areaw'):
            a = status(e)['area'][:2]
            if self.tm_area is not None and a != self.tm_area:
                self.tm = 40
            self.tm_area = a
            if self.tm > 0:
                self.tm -= 1
                if self.tm % 2 == 1:
                    k = (k or 0) | S
        else:
            self.tm_area, self.tm = None, 0
        return k

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        if kind in RUN3_KINDS:
            return Run3Driver.step(self, e)
        if kind == 'retry_here3':
            if self.retry_k != self.k - g[1]:
                self.tries = 0
            self.retry_k = self.k - g[1]
            self.next_goal()
            return None
        if kind == 'die1':
            # ('die1', tag): once per tag - after the Continue the stretch replays past it
            # (the game over itself is ShipDriver2's: its retry jump to the checkpoint's Continue)
            if g[1] in self.died and getattr(self, 'dying', None) != g[1]:
                self.next_goal()
                return None
            self.died.add(g[1])
            self.dying = g[1]
            j = self.k
            # no enemy to walk into for 900 field frames (a save point's area): the death is skipped (logged)
            from ship_route2 import enemies
            s = status(e)
            # (also an enemy the walk cannot reach: 6000 frames in the goal without the game over)
            if s['p'] is not None and s['scene'] == 0 and (not [o for o in enemies(e) if o[4] > 0] or self.t > 6000):
                self.no_en = getattr(self, 'no_en', 0) + 1
                if self.no_en > 900:
                    print('die1: no enemy, skipped', g[1], s, flush=True)
                    self.no_en, self.dying = 0, None
                    self.died.discard(g[1])
                    self.skipped = getattr(self, 'skipped', set()) | {g[1]}
                    self.died.add(g[1])
                    self.next_goal()
                    return None
            else:
                self.no_en = 0
            self.goals[j] = ('die',)
            try:
                r = super().step(e)
            finally:
                self.goals[j] = g
            if self.k != j:
                self.dying = None
            return r
        if kind == 'navonce':
            # ('navonce', tag, w, a): into area w-a for the planned death `tag` (skipped once it happened)
            if g[1] in self.died:
                self.next_goal()
                return None
            j = self.k
            self.goals[j] = ('nav', g[2], g[3], False)
            try:
                return super().step(e)
            finally:
                self.goals[j] = g
        if kind == 'levelup1':
            self.goals[self.k] = ('grind', level_of(e) + 1, g[1])
            self.grind_k = None
            return None
        if kind == 'opensave':
            s = status(e)
            if s['scene'] > 0 or s['p'] is None:
                self.ow = getattr(self, 'ow', 0) + 1
                if self.ow >= g[1]:
                    self.ow = 0
                    self.next_goal()
                    return None
                return 0
            self.ow = 0
            if self.t > 2000:
                raise RuntimeError(f'opensave: no menu {s}')
            ph = self.t % 80
            if ph < 3:
                return A
            if 40 <= ph < 41 and self.t > 80:
                return KEY.LEFT if (self.t // 160) % 2 else KEY.RIGHT
            if 50 <= ph < 53 and self.t > 80:
                return KEY.UP
            return 0
        return super().step(e)


def stressed(goals):
    out = []
    # (the save points' areas have no enemy at hand: Goku walks into 20-5's robots first; inside Buu the next area)
    deaths = {'b03_14-1': ('levelup', [(14, 1), (14, 2)]), 'c02_goku124': ('nav', (20, 5)),
              'd00_21-1': ('die', None), 'd01_21-14': ('die', None)}
    prev = None
    for j, g in enumerate(goals):
        kind = g[0] if isinstance(g, tuple) and g else None
        if kind in FIGHTS and prev not in FIGHTS:
            out.append(('pausespin', 3))
        out.append(g)
        if kind == 'mark' and g[1] in deaths:
            how, areas = deaths[g[1]]
            # (after the checkpoint's luck punch: the retry stretch is armed)
            out.append(('_after_luck', g[1]))
        if kind in ('events', 'eventsw'):
            nxt = goals[j + 1][0] if j + 1 < len(goals) and isinstance(goals[j + 1], tuple) else None
            if nxt not in FIGHTS and nxt not in ('evade', 'events', 'eventsw'):
                out += [('mashkeys', S, 2, 60), ('bout',)]
        if kind is not None:
            prev = kind
    # place the planned deaths right after the luck goal that follows their mark
    res = []
    pend = None
    for g in out:
        if isinstance(g, tuple) and g and g[0] == '_after_luck':
            pend = g[1]
            continue
        res.append(g)
        if pend and g == ('luck',):
            how, areas = deaths[pend]
            if how == 'levelup':
                res += [('levelup1', areas), ('die1', pend)]
            elif how == 'nav':
                res += [('navonce', pend, areas[0], areas[1]), ('die1', pend)]
            else:
                res += [('die1', pend)]
            pend = None
    return res


def goals_run3(e):
    sbroute2.checkpoint = checkpoint3
    g = sbroute2.goals_run2(e)
    # the end (scope of 2026-10-06: Kid Buu -> credits -> post-game are the Earth agent's): out of Inside Buu after the
    # brain fight - the first frames in 20-8 with the leader controllable; the last save is 21-25's checkpoint
    # (before the brain), the slot the next part Continues
    k = g.index(('mark', 'd03_kid_buu_saga'))
    g = g[:k + 1] + [('until', 20, 8), ('wait', 30), ('mark', 'e00_end_20-8')]
    return [('menutour',)] + stressed(g)


COVERAGE = """
Super / Kid Buu (b3_superbuu): run 2's part (HFIL save, the Lookout / Gotenks, Ultimate Gohan's Pyramid grind with
saves, two Z-Mart curry runs, a save + soft reset + Continue at 4-1, the crater, the wasteland, Goku's 20-6 grind,
Vegeta inside Buu, Goku in the brain, Kid Buu with food and retries) plus: the switch screen in and out at every
checkpoint, the pause pages after every Continue, the pause spun before every boss fight, START on alternate frames
as every scripted stretch lets go and through every area change of a walk between areas, three planned game overs ->
Continue (the Pyramid right after a level-up, Goku at 20-6, inside Buu at 21-1); ends saved at 4-1, standing in
Capsule Corp 4-20 before the final sequence
"""


def run(st, ee, out, cap):
    e = start_emu(str(Path(st).resolve()))
    sbemu.sd_write(e, Path(ee).read_bytes())
    goals = goals_run3(e)
    d = SbDriver3(goals)
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
    print(f'b3_superbuu: {len(log)} frames {time.time() - t0:.0f}s', status(e), 'planned deaths', sorted(d.died),
          flush=True)


def verify(st, ee, out):
    e = start_emu(str(Path(st).resolve()))
    sbemu.sd_write(e, Path(ee).read_bytes())
    for k in json.loads((out / 'keys.json').read_text())['keys']:
        e.set_keys(k)
        e.run(1)
    e.set_keys(0)
    ref = start_emu(str(out / 'end.ss'))
    same = e.ewram() == ref.ewram() and e.iwram() == ref.iwram() and sbemu.sd_read(e) == (out / 'end.eeprom').read_bytes()
    print('verify:', 'byte-identical' if same else 'DIFFERS', status(e))
    return same


def main():
    a = sys.argv[1:]
    st, ee, out = a[0], a[1], Path(a[2])
    out.mkdir(parents=True, exist_ok=True)
    if '--verify' in a:
        sys.exit(0 if verify(st, ee, out) else 1)
    run(st, ee, out, int(a[a.index('--cap') + 1]) if '--cap' in a else 1500000)


if __name__ == '__main__':
    main()
