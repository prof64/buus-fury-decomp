"""Run 3's ship chapter (`s3_ship`; engine/docs/segments.md "Run 3"): run 2's ship goals (re/buu/ship/ship_route2.py
goals_run2: 7-2 after the tournament -> Southwest Forest -> Babidi's ship -> Majin Buu, with its Save / Continue /
soft-reset stretches) with run 3's stresses on top, driven by ShipDriver3 = ShipDriver2 + Run3Driver's goals
('pausespin', 'mashkeys', 'menutour', ...). Input only.

Stresses (COVERAGE below):
- the pause menu spun (opened / closed at the fastest accepted rate) before every ship fight ('fightp'; not Yakon's
  dark room, whose goal starts at the door)
  and after every world-map landing ('fly')
- every pause page both ways at the chapter's start and after each Continue
- START on alternate frames for 60 frames after each 'story' / 'events' goal's end (the frames a script lets go)
- every row of the three Papaya Island stalls bought once while the zenie lasts, the refusal on the dearest (shops3's
  'shopenter' / 'buyrefused' / 'listout', 'buyeach' here)
- the East District game over after the 8-1 Save comes right after a level-up (one level gained first, 'levelup1')

play: tools/run3/ship3.py START.ss START.eeprom OUTDIR [--cap N] - plays goals_run3 from that state (the cartridge
save loaded first; one session: the Save / Continue stretches need the flash), OUTDIR/keys.json + end.ss / end.eeprom.
"""
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
from ship_route2 import ShipDriver2, goals_run2, KEY, DEATH_CONTINUE  # noqa: E402
from run3_driver import Run3Driver  # noqa: E402
from run2_driver import Run2Driver  # noqa: E402
from route import start_emu, status  # noqa: E402
from ship_driver import level_of  # noqa: E402  (r1frozen's, as ship_route2 imports it)
from shops3 import ShopDriver, shop_list, zenie, mode_vt, shop_sel, VT_BUY  # noqa: E402

SHOP_KINDS = ('shopenter', 'buyrefused', 'listout')

S, A = KEY.START, KEY.A
RUN3_KINDS = ('mashkeys', 'pausespin', 'menutour', 'textmash')


class ShipDriver3(ShipDriver2):
    """ShipDriver2 with Run3Driver's stress goals (its handlers called as plain functions: every other goal stays
    ShipDriver2's, its fallbacks Driver's) and Run2Driver's 'bout' (B until the field runs, which the spins end with)"""
    idle = 0
    tap = ShopDriver.tap

    def step(self, e):
        kind = self.goals[self.k][0]
        if kind in RUN3_KINDS:
            return Run3Driver.step(self, e)
        if kind == 'bout':
            return Run2Driver.step(self, e)
        if kind == 'titlecont':
            # after the death -> title -> Options -> Continue program: until the field of (w, a) runs; while it does
            # not, the title's way back in again (B out of whatever is open, UP to Continue, A through the slot list
            # and the load) - run 3's title timing differs from run 2's (the program's B fell in a fade at 8-41)
            g = self.goals[self.k]
            s = status(e)
            if s['area'][:2] == [g[1], g[2]] and s['p'] is not None:
                self.next_goal()
                return None
            if self.t > 20000:
                raise RuntimeError(f'titlecont: {g} not reached {s}')
            prog = [KEY.B] * 3 + [0] * 80 + [KEY.B] * 3 + [0] * 80 + [KEY.UP] * 3 + [0] * 40 + [KEY.A] * 3 + \
                [0] * 200 + [KEY.A] * 4 + [0] * 120 + [KEY.A] * 4 + [0] * 400
            if self.t < 1500:
                return 0
            return prog[(self.t - 1500) % len(prog)]
        if kind == 'levelup1':
            # ('levelup1', areas): the grind until the active character has gained one level (then a 'die' follows:
            # the game over right after a level-up's popup)
            self.goals[self.k] = ('grind', level_of(e) + 1, self.goals[self.k][1])
            self.grind_k = None
            return None
        if kind == 'buyeach':
            # on a stall's Buy screen: every row once, top to bottom, while the zenie covers it (shops3's 'buyall'
            # first pass only: the zenie is kept for the chapter's later stalls)
            g = self.goals[self.k]
            lst = shop_list(e)
            z = zenie(e)
            if mode_vt(e) != VT_BUY:
                raise RuntimeError(f'buyeach: not on the Buy screen {status(e)}')
            st = getattr(self, 'buy', None)
            if st is None:
                st = self.buy = {'i': 0, 'z': z}
            if z < st['z']:
                st['z'] = z
                st['i'] += 1
                st['mv'] = None
                self.t = 0
                return 0
            while st['i'] < len(lst) and lst[st['i']]['price'] > z:
                st['i'] += 1
            if st['i'] >= len(lst):
                self.buy = None
                self.next_goal()
                return None
            target = st['i']
            sel = shop_sel(e)
            mv = st.get('mv')
            if mv is not None:
                mv['t'] += 1
                if mv['t'] <= 3:
                    return mv['k']
                if sel == mv['from'] and mv['t'] < 40:
                    return 0
                if mv.get('q', 0) < 10:
                    mv['q'] = mv.get('q', 0) + 1
                    return 0
                st['mv'] = None
                self.t = 0
                return 0
            if sel != target:
                st['mv'] = {'k': KEY.DOWN if sel < target else KEY.UP, 't': 0, 'from': sel}
                return 0
            if self.t > 2000:
                raise RuntimeError(f'buyeach: no purchase {target} {lst[target]} zenie {z} {status(e)}')
            return KEY.A if self.t % 40 < 3 else 0
        if kind in SHOP_KINDS:
            return ShopDriver.step(self, e)
        return ShipDriver2.step(self, e)


# the three Papaya Island stalls (s257 drinks 6 rows, s258 food 5, s259 trinkets: 8 equipment + two one-offs at 25,000
# / 50,000): every row bought once while the zenie lasts, the refusal on the dearest, out
STALLS = [(188, 712), (276, 592), (188, 472)]


def every_stall():
    out = []
    for x, y in STALLS:
        out += [('unstick',), ('walk', x, y), ('face', 'U'), ('shopenter', 0), ('buyeach',), ('buyrefused',),
                ('listout',), ('bout',), ('wait', 30)]
    return out


def stressed3(goals):
    out = []
    died = False
    shop_at = None
    for j, g in enumerate(goals):
        kind = g[0] if isinstance(g, tuple) and g else None
        # the Continue's arrival: closed-loop ('titlecont') instead of a plain wait for the area
        # Gohan carries the ship's stage 3: run 3's Earth left him weaker than run 2's (ten game overs at 8-41 with
        # run 2's level 100) - grind him further
        if kind == 'grind' and g[1] == 100:
            out.append(('grind', 125) + tuple(g[2:]))
            continue
        if kind == 'until' and j > 0 and goals[j - 1] is DEATH_CONTINUE:
            out.append(('titlecont', g[1], g[2]))
            continue
        if kind == 'fightp':
            out.append(('pausespin', 3))
        # the East District death after the 8-1 Save: one level gained first (the game over comes right after a
        # level-up's popup)
        if kind == 'die' and not died:
            died = True
            # (then out and back in: an area reload brings its enemies back for the death)
            out += [('levelup1', [(2, 9), (2, 10)]), ('nav', 2, 8, False), ('nav', 2, 9, False)]
        out.append(g)
        # every stall row once, after run 2's own stall stop (before leaving Papaya Island for the ship)
        if g == ('mark', 'r05_shop'):
            shop_at = len(out)
        if kind == 'fly':
            out.append(('pausespin', 2))
        if kind in ('story', 'events'):
            out.append(('mashkeys', S, 2, 60))
            out.append(('bout',))
        # (after run 2's stall goals: the first nav away from 7-2 once the shop mark passed)
        if shop_at is not None and kind == 'nav' and g[1:3] == (7, 1):
            out[-1:-1] = every_stall()
            shop_at = None
    return out


def goals_run3(e):
    g = goals_run2(e)
    return [('menutour',)] + stressed3(g)


COVERAGE = """
Ship (s3_ship): run 2's ship chapter (Papaya Island's shops and talkers, the Lookout / Korin, the 8-1 Save -> a death
in East District -> Continue, the grinds with Energy Blasts, the stalls, the ship with 8-28's Save + soft reset +
Continue and the 8-41 checkpoints) plus: the pause pages both ways first; the pause spun before every boss / ship
fight and after every landing; START on alternate frames for 60 frames as every scripted stretch lets go; every
Papaya Island stall row bought once + the refusals; the East District game over right after a level-up
"""


def main():
    a = sys.argv[1:]
    st, ee, out = a[0], a[1], Path(a[2])
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 900000
    out.mkdir(parents=True, exist_ok=True)
    e = start_emu(str(Path(st).resolve()))
    sbemu.sd_write(e, Path(ee).read_bytes())
    goals = goals_run3(e)
    d = ShipDriver3(goals)
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
            s = status(e)
            if (s['area'][:2], d.k) != prev:
                prev = (s['area'][:2], d.k)
                print(' ', len(log), d.k, str(d.goals[d.k] if d.k < len(d.goals) else '')[:80], s, flush=True)
    except Exception as ex:  # noqa: BLE001 - keep what ran, show where
        e.save_file(str(out / 'fail.ss'))
        (out / 'fail.eeprom').write_bytes(sbemu.sd_read(e))
        (out / 'keys.json').write_text(json.dumps(log))
        print(f'FAIL after {len(log)} frames at goal {d.k} {d.goals[d.k] if d.k < len(d.goals) else ""}: {ex}',
              status(e), flush=True)
        sys.exit(1)
    (out / 'keys.json').write_text(json.dumps(log))
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    (out / 'end.eeprom').write_bytes(sbemu.sd_read(e))
    print(f's3_ship: {len(log)} frames {time.time() - t0:.0f}s', status(e), flush=True)


if __name__ == '__main__':
    main()
