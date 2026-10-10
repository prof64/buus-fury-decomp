"""Run 2's driver: earth_driver.EarthDriver plus
  ('intro', period)     the title -> New Game (route.intro_keys' title part), then the prologue's text boxes with A
                        pressed 4 of every `period` frames, up to route.INTRO_FRAMES frames from the reset
  ('keys', [(k, n), ..]) a fixed key program (k held n frames)
  ('die', cap)          stand still (no keys) until the leader is gone and the game-over text runs, then A through
                        "You have died!" until the title screen (area 0-0, no leader)
  ('continue',)         title -> Single Player -> Game 1 (the save slot): START every 40 frames until a leader exists,
                        then A until it is controllable (no event) - the game's Continue / slot-load path
  ('save',)             at a save point's touch spot, facing it: the Save Game screen opens itself; A = Game 1, then A
                        through "Game Saved" (the EEPROM write) until the field runs again
  ('reset',)            soft reset (A+B+START+SELECT 10 frames, then 5 free)
  ('levelto', lvl, cycle) as 'levelup', until the current character reaches level `lvl`
All closed-loop on the emulator state (deterministic: the key log replays exactly)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field'):
    sys.path.insert(0, str(ROOT / d))
from earth_driver import EarthDriver  # noqa: E402
from route import KEY, status, INTRO_START, INTRO_FRAMES  # noqa: E402


class Run2Driver(EarthDriver):
    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        if kind == 'intro':
            f = self.t
            if f >= INTRO_FRAMES:
                self.next_goal()
                return None
            if f < INTRO_START - 20:
                if f % 40 < 4:
                    return KEY.START if (f // 40) % 2 == 0 else KEY.A
                return 0
            if f < INTRO_START:
                return 0
            return KEY.A if (f - INTRO_START) % g[1] < 4 else 0
        if kind == 'keys':
            t = self.t
            for k, n in g[1]:
                if t < n:
                    return k
                t -= n
            self.next_goal()
            return None
        if kind == 'bout':
            # B every 30 frames until the field runs again (no event, a leader) for 30 frames (menus, shop lists)
            s = status(e)
            if s['scene'] == 0 and s['p'] is not None:
                self.idle += 1
                if self.idle >= 30:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            if self.t > 3000:
                raise RuntimeError(f'bout: still in a menu after 3000 frames: {s}')
            return KEY.B if self.t % 30 < 3 else 0
        if kind == 'reset':
            if self.t < 10:
                return KEY.A | KEY.B | KEY.START | KEY.SELECT
            if self.t < 15:
                return 0
            self.next_goal()
            return None
        s = status(e)
        if kind == 'die':
            if self.t > g[1]:
                raise RuntimeError(f'die: not dead after {g[1]} frames: {s}')
            if not getattr(self, 'died', False):
                if s['hp'] == 0 and s['p'] is None:
                    self.died = True
                return 0
            # A through the game-over text until the title (no leader, area 0-0; no save yet) or until the leader is
            # back at the last save point (the game reloads the slot itself; no A then - it would open the save menu)
            title = s['p'] is None and s['scene'] == 0 and s['area'][0] == 0
            back = s['p'] is not None and s['hp'] > 0 and s['scene'] == 0
            if title or back:
                self.idle += 1
                if self.idle > (240 if title else 60):
                    self.died = False
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            return KEY.A if self.t % 40 < 3 else 0
        if kind == 'newgame':
            # title -> Single Player -> Game 1 (empty: New Game): START / A alternately every 40 frames until the
            # prologue runs (an event), then its text with A 4 of every g[1] frames until the leader walks in 1-1
            if not getattr(self, 'ng_started', False):
                if s['scene'] > 0 or s['area'][:2] not in ([0, 0], [0, 2]):
                    self.ng_started = True
                elif self.t % 40 < 4:
                    return KEY.START if (self.t // 40) % 2 == 0 else KEY.A
                else:
                    return 0
            if s['area'][:2] == [1, 1] and s['p'] is not None and s['scene'] == 0:
                self.idle += 1
                if self.idle >= 60:
                    self.ng_started = False
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            return KEY.A if self.t % g[1] < 4 else 0
        if kind == 'continue':
            if s['p'] is None:
                self.idle = 0
                # START on the title, A on Single Player / Game 1
                if self.t % 40 < 4:
                    return KEY.START if (self.t // 40) % 2 == 0 else KEY.A
                return 0
            if s['scene'] == 0:
                self.idle += 1
                if self.idle >= 20:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            return KEY.A if self.t % 20 < 3 else 0
        if kind == 'save':
            # the save screen hides the leader (p None) while it is up
            if self.t < 2:
                return KEY.UP
            if self.t < 60:
                return 0
            if self.t < 63:
                return KEY.A
            if s['p'] is None or s['scene'] > 0:
                self.idle = 0
                return KEY.A if self.t % 40 < 3 else 0
            self.idle += 1
            if self.idle >= 30:
                self.next_goal()
                return None
            return 0
        if kind == 'levelto':
            import json as _json
            from common_field import record_of
            if not hasattr(self, 'xp_table'):
                from grid import CONTENT
                self.xp_table = [x['total_xp'] for x in _json.loads((CONTENT / 'levels.json').read_text())['levels']]
            xp = record_of(e)['xp']
            lvl = max(i for i in range(len(self.xp_table)) if xp >= self.xp_table[i])
            if lvl >= g[1]:
                self.next_goal()
                return None
            self.goals[self.k:self.k] = list(g[2])
            self.k -= 1
            self.next_goal()
            return None
        return super().step(e)
