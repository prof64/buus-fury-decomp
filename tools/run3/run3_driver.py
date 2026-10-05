"""Run 3's driver (the adversarial playthrough, engine/docs/segments.md "Run 3"): run 2's Run2Driver plus
  ('mashkeys', key, period, n)        `key` for 1 frame every `period` frames, nothing in between, for n frames
  ('pushmash', dir, extra, period, n) hold `dir` (into an exit) and OR `extra` every `period` frames for n frames
                                      (through the transition), then close whatever opened (B until the field runs)
  ('pausespin', n)                    open and close the pause menu n times (START, then START as soon as the menu
                                      takes input again - it ignores a press while it fades)
  ('menutour',)                       the pause menu's pages: R through all of them and L back, then closed
  ('textmash', period, until)         A every `period` frames (alternate frames: period 2) until `until` (an area
                                      key (w, a) reached with the leader controllable)
All closed-loop on the emulator state (deterministic: the key log replays exactly)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/run2'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
from run2_driver import Run2Driver  # noqa: E402
from route import KEY, status  # noqa: E402


def menu_open(e):
    """a menu screen is up (the field's leader is hidden behind it): status p None while the scene is 0"""
    s = status(e)
    return s['p'] is None and s['scene'] == 0


class Run3Driver(Run2Driver):
    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        if kind == 'mashkeys':
            if self.t >= g[3]:
                self.next_goal()
                return None
            return g[1] if self.t % g[2] == 0 else 0
        if kind == 'pushmash':
            if self.t == 0:
                self.push_area = status(e)['area'][:2]
            if self.t < g[4]:
                # (the direction only until the area changes: in the new one, just the extra key through the fade-in)
                moved = status(e)['area'][:2] != self.push_area
                return (0 if moved else g[1]) | (g[2] if self.t % g[3] == 0 else 0)
            # close whatever the presses opened, as 'bout'; if the pause it opened stopped the walk-in before the exit
            # (still in the same area), push again without the extra key until the area changes
            self.goals[self.k:self.k + 1] = [('bout',), ('pushagain', g[1], self.push_area)]
            self.t = 0
            return 0
        if kind == 'skillnext':
            # L (2 frames on, 12 off) until the selected skill slot moves on from the one at the goal's start (a single
            # L press can fall into the frames after a transition / pause that ignore it)
            rec = 0x03001560 + 0x68 * e.r8(0x03001068)
            if self.t == 0:
                self.skill0 = e.r8(rec + 0x15)
            if e.r8(rec + 0x15) != self.skill0 and self.t % 14 == 0:
                self.next_goal()
                return None
            if self.t > 600:
                raise RuntimeError(f'skillnext: the slot did not move {status(e)}')
            return KEY.L if self.t % 14 < 2 else 0
        if kind == 'skillslot':
            # L (2 frames on, 12 off) until the selected skill slot is g[1]
            rec = 0x03001560 + 0x68 * e.r8(0x03001068)
            if e.r8(rec + 0x15) == g[1] and self.t % 14 == 0:
                self.next_goal()
                return None
            if self.t > 600:
                raise RuntimeError(f'skillslot: slot {g[1]} not reached {status(e)}')
            return KEY.L if self.t % 14 < 2 else 0
        if kind == 'pushagain':
            s = status(e)
            if s['area'][:2] != g[2]:
                self.next_goal()
                return None
            if self.t > 600:
                raise RuntimeError(f'pushagain: no exit {g} {s}')
            if s['p'] is None or s['scene'] > 0:
                return 0
            return g[1]
        if kind == 'pausespin':
            # phases of one spin: press START (open), wait until the menu is up, START (close), wait until the field
            st = getattr(self, 'spin', None)
            if st is None:
                st = self.spin = {'n': 0, 'ph': 0, 'w': 0}
            if st['n'] >= g[1]:
                self.spin = None
                self.goals[self.k] = ('bout',)
                self.t = 0
                return 0
            if status(e)['scene'] > 0:
                # (an event runs: play it through first - the pause cannot open under it)
                st['w'] = 0
                return self.mash_a()
            up = menu_open(e)
            st['w'] += 1
            if st['w'] > 600:
                raise RuntimeError(f'pausespin: stuck in phase {st["ph"]} {status(e)}')
            if st['ph'] == 0:
                if up:
                    st['ph'], st['w'] = 1, 0
                    return 0
                return KEY.START if st['w'] % 2 == 1 else 0
            if not up:
                st['ph'], st['w'] = 0, 0
                st['n'] += 1
                return 0
            return KEY.START if st['w'] % 2 == 1 else 0
        if kind == 'menutour':
            prog = [KEY.START] + [0] * 40
            for k in [KEY.R] * 5 + [KEY.L] * 5:
                prog += [k] + [0] * 24
            prog += [KEY.B] + [0] * 30
            if self.t < len(prog):
                return prog[self.t]
            self.goals[self.k] = ('bout',)
            self.t = 0
            return 0
        if kind == 'textmash':
            s = status(e)
            w, a = g[2]
            if s['area'][:2] == [w, a] and s['p'] is not None and s['scene'] == 0:
                self.idle += 1
                if self.idle >= 60:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            if self.t > 30000:
                raise RuntimeError(f'textmash: {g} not reached {s}')
            return KEY.A if self.t % g[1] == 0 else 0
        return super().step(e)
