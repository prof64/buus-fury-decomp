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

S, A = KEY.START, KEY.A
RUN3_KINDS = ('mashkeys', 'pausespin', 'menutour', 'textmash')


class ShipDriver3(ShipDriver2):
    """ShipDriver2 with Run3Driver's stress goals (its handlers called as plain functions: every other goal stays
    ShipDriver2's, its fallbacks Driver's) and Run2Driver's 'bout' (B until the field runs, which the spins end with)"""
    idle = 0

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
        return ShipDriver2.step(self, e)


def stressed3(goals):
    out = []
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
        out.append(g)
        if kind == 'fly':
            out.append(('pausespin', 2))
        if kind in ('story', 'events'):
            out.append(('mashkeys', S, 2, 60))
            out.append(('bout',))
    return out


def goals_run3(e):
    g = goals_run2(e)
    return [('menutour',)] + stressed3(g)


COVERAGE = """
Ship (s3_ship): run 2's ship chapter (Papaya Island's shops and talkers, the Lookout / Korin, the 8-1 Save -> a death
in East District -> Continue, the grinds with Energy Blasts, the stalls, the ship with 8-28's Save + soft reset +
Continue and the 8-41 checkpoints) plus: the pause pages both ways first; the pause spun before every boss / ship
fight and after every landing; START on alternate frames for 60 frames as every scripted stretch lets go
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
