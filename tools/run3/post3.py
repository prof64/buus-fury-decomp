"""Run 3's post-game + credits (`p3_post`; engine/docs/segments.md "Run 3", item 4): from Capsule Corp 4-20 after Kid
Buu (the part before saves at West City 4-1 before the fight) - what the game leaves open before the ending, a save,
a soft reset -> Continue of that cleared save, the final sequence with its text mashed, the credits with inputs held,
the post-credits title's menus. Driven by PostDriver = shops3.ShopDriver (Run3Driver + the shop goals) + the goals
below. Input only.

  ('optcycle',)       pause -> Options: every slider (Sound FX, Music, Text Speed) to 0, to 8, back to its value,
                      closed-loop on the options word 0x03001914 (text speed, SFX, music), then out
  ('optcycle', plan)  the same with the rows' targets given: [(byte, [values]) per row in page order; [] = pass]
                      (textspeed(v): the Text Speed row only)
  ('endmash', n)      A on alternate frames (one frame on, one off) until the credits run (area 0-2 with a scene after
                      the Tournament 7-6 has been seen), at most n frames
  ('credits',)        through the credits a held key in turns (CREDIT_HOLDS, each HOLD frames) until the title is up
                      (area 0-2, no scene, no leader) for 120 frames
  ('titlekeys',)      the title's menus with keys on alternate frames: Options (every row touched), back; Start ->
                      Single Player -> the slot list (the cleared save) -> back out; ends on the title

play: tools/run3/post3.py START.ss START.eeprom OUTDIR [--cap N] [--trace] - the goals from that state (the cartridge
save loaded first; one session), OUTDIR/keys.json + end.ss / end.eeprom."""
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 'tools/run2', 'tools/run3'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
import sbemu  # noqa: E402  (the savedata-capable emulator lib first)
from route import start_emu, status, KEY  # noqa: E402
from shops3 import ShopDriver  # noqa: E402

U, D, L, R, A, B, S, E = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A, KEY.B, KEY.START, KEY.SELECT
LS, RS = KEY.L, KEY.R
VT_OPTIONS = 0x080498F0  # the pause menu's Options page (the mode object 0x03000F90)
OPTS = 0x03001914           # text speed, SFX volume, music volume (0..8)
HOLD = 600
# held through the credits, in turns (never A+B+START+SELECT together: the soft reset)
CREDIT_HOLDS = [A, S, U, D, L, R, S | E, A | B, LS | RS, U | L, D | R, B, E, 0]


class PostDriver(ShopDriver):
    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        if kind == 'optcycle':
            return self.optcycle(e)
        if kind == 'endmash':
            s = status(e)
            if s['area'][:2] == [7, 6]:
                self.seen76 = True
            if getattr(self, 'seen76', False) and s['area'][:2] == [0, 2] and s['scene'] > 0 and s['p'] is None:
                self.next_goal()
                return None
            if self.t > g[1]:
                raise RuntimeError(f'endmash: no credits after {g[1]} frames {s}')
            return A if self.t % 2 == 0 else 0
        if kind == 'credits':
            s = status(e)
            if s['area'][:2] == [0, 2] and s['scene'] == 0 and s['p'] is None:
                self.idle += 1
                if self.idle >= 120:
                    self.next_goal()
                    return None
                return 0
            self.idle = 0
            if self.t > 40000:
                raise RuntimeError(f'credits: no title after 40000 frames {s}')
            return CREDIT_HOLDS[(self.t // HOLD) % len(CREDIT_HOLDS)]
        if kind == 'titlekeys':
            prog = []
            # Options (DOWN, A), each row: RIGHT / LEFT / DOWN; B out; Start (UP, A) -> Single Player (A) -> the slot
            # list -> DOWN / UP -> B, B back to Start / Options
            for k in [D, A, R, L, D, R, L, D, R, L, D, R, L, D, B, B, U, A, A, D, U, D, U, B, B, D, U]:
                prog += [k, 0] + [0] * 28
            prog += [0] * 120
            if self.t < len(prog):
                return prog[self.t]
            self.next_goal()
            return None
        return super().step(e)

    def optcycle(self, e):
        """open the pause, R to Options (Status -> Inventory -> Equip -> Options), then per row its slider to 0, to 8,
        back to the start value; B out of the rows, B to close (then 'bout' if anything is left open)"""
        st = getattr(self, 'oc', None)
        if st is None:
            vals = [e.r8(OPTS + k) for k in range(3)]
            # the page's rows: Sound FX (byte 1), Music (byte 2), Text Speed (byte 0)
            g = self.goals[self.k]
            plan = g[1] if len(g) > 1 else [(1, [0, 8, vals[1]]), (2, [0, 8, vals[2]]), (0, [0, 8, vals[0]])]
            st = self.oc = {'ph': 'open', 'w': 0, 'row': 0, 'tgt': 0, 'plan': plan}
        st['w'] += 1
        if st['w'] > 4000:
            raise RuntimeError(f'optcycle stuck {st} {status(e)}')
        t = st['w']
        if st['ph'] == 'open':
            # the field first (an arrival's walk-in ignores START), then START until the pause is up
            from run3_driver import menu_open
            s = status(e)
            if menu_open(e) and st.get('ready'):
                st['ph'], st['w'] = 'pages', 0
                return 0
            if s['p'] is not None and s['scene'] == 0:
                st['ready'] = st.get('ready', 0) + 1
            if st.get('ready', 0) > 20 and t % 60 < 2:
                return S
            return 0
        if st['ph'] == 'pages':
            # R (one frame) every 60 frames until the Options page is the mode (a press during a page's build is lost)
            from shops3 import mode_vt
            if mode_vt(e) == VT_OPTIONS:
                st['ph'], st['w'] = 'settle', 0
                return 0
            return RS if t % 60 == 59 else 0
        if st['ph'] == 'settle':
            if t >= 40:
                st['ph'], st['w'] = 'row', 0
            return 0
        if st['ph'] == 'row':
            if st['row'] >= len(st['plan']):
                st['ph'], st['w'] = 'close', 0
                return 0
            byte, tgts = st['plan'][st['row']]
            if st['tgt'] >= len(tgts):
                st['row'] += 1
                st['tgt'] = 0
                st['ph'], st['w'] = 'down', 0
                return 0
            v = e.r8(OPTS + byte)
            want = tgts[st['tgt']]
            if v == want:
                st['tgt'] += 1
                st['w'] = 0
                return 0
            if t % 16 == 0:
                return L if v > want else R
            return 0
        if st['ph'] == 'down':
            prog = [D] + [0] * 30
            if t <= len(prog):
                return prog[t - 1]
            st['ph'], st['w'] = 'row', 0
            return 0
        if st['ph'] == 'close':
            self.oc = None
            self.goals[self.k] = ('bout',)
            self.t = 0
            return 0
        return 0


# --- West City 4-1 / Capsule Corp 4-20 / the Z-Mart 0-92 (Earth-route coordinates; the Z-Mart's floor is 0-91's)
TO_4_1 = [('walk', 4, 390), ('hold', L, 30), ('area', 4, 1), ('wait', 30)]
TO_4_20 = [('walk', 850, 360), ('hold', R, 30), ('area', 4, 20), ('wait', 30)]
def textspeed(v):
    return ('optcycle', [(1, []), (2, []), (0, [v])])


# West City's talkers with the Text Speed changed between the talks (slowest, fastest, middle - the pause cannot open
# under a dialogue: the soak agent's "Options mid-dialogue" became "between two dialogues"); the second and fourth
# talks' boxes mashed on alternate frames
TALKS_4_1 = [('talkto', 320, 360, 'D'), textspeed(0), ('talkto', 416, 128, 'D'), textspeed(8),
             ('talkto', 784, 360, 'D'), textspeed(4), ('talkto', 664, 544, 'D')]
ZMART_WEST = [
    ('walk', 264, 320), ('hold', U, 40), ('area', 0, 92), ('wait', 30), ('walk', 100, 110), ('hold', L, 20),
    ('shopenter', 0, 0), ('buyall',), ('buyrefused',), ('listout',),
    ('shopenter', 0, 1), ('buyall',), ('buyrefused',), ('listout',),
    ('bout',),
    ('hold', L, 10), ('shopenter', 1), ('sellall', 3), ('listout',),
    ('bout',),
    ('walk', 116, 200), ('hold', D, 30), ('area', 4, 1), ('wait', 30),
]
SAVE_4_1 = [('walk', 148, 594), ('save',), ('walk', 148, 630), ('wait', 20)]
# Hercule City by the world map: the circus's fighters are still there (flag 25 set); its talkers; back to West City
HERCULE = [
    ('walk', 410, 356), ('talk', 'U'), ('wait', 60), ('fly', 2441, 1065, 127, 135), ('wait', 30),
    ('talkto', 208, 112, 'D'), ('wait', 20),
    ('walk', 500, 10), ('hold', U, 30), ('area', 3, 2), ('wait', 30),
    ('talkto', 224, 88, 'D'), ('wait', 20),
    ('walk', 4, 140), ('hold', L, 30), ('area', 3, 14), ('wait', 30),
    ('walk', 400, 8), ('hold', U, 30), ('area', 3, 15), ('wait', 30), ('pausespin', 2), ('fight2', 2500), ('wait', 30),
    ('walk', 160, 8), ('hold', U, 30), ('area', 3, 17), ('wait', 30), ('fight2', 2500), ('wait', 30),
    ('walk', 160, 290), ('hold', D, 30), ('area', 3, 15), ('wait', 30),
    ('walk', 160, 290), ('hold', D, 30), ('area', 3, 14), ('wait', 30),
    ('walk', 400, 48), ('walk', 450, 140), ('hold', R, 30), ('area', 3, 2), ('wait', 30),
    ('walk', 520, 210), ('hold', D, 30), ('area', 3, 1), ('wait', 30),
    ('walk', 40, 150), ('talk', 'L'), ('wait', 60), ('fly', 1193, 1113, 119, 183), ('wait', 30),
]
# the final sequence: Capsule Corp 4-20 -> 4-21's lobby (s190: "Entering this area will start the final sequence"
# - Yes), the ending mashed, the credits held, the title
FINAL = [
    ('walk', 232, 64), ('hold', U, 30), ('area', 4, 21), ('wait', 30),
    ('walk', 100, 90), ('hold', U, 60), ('endmash', 30000), ('credits',), ('titlekeys',),
]

GOALS = ([('wait', 20), ('menutour',), ('talkto', 232, 248, 'D'), ('wait', 20), ('talkto', 128, 136, 'D'),
          ('wait', 20)] + TO_4_1 + TALKS_4_1 + ZMART_WEST + [('optcycle',)] + SAVE_4_1 + HERCULE + SAVE_4_1 +
         [('reset',), ('continue',), ('wait', 30), ('pausespin', 3)] + TO_4_20 + FINAL)

COVERAGE = """
Post-game + credits (p3_post): Capsule Corp's talkers (s184 / s185) and the pause pages both ways; West City's talkers;
the West City Z-Mart (0-92): every Items row and every Equipment row bought while the zenie lasts, the refusals,
three rows sold; Options: Sound FX / Music / Text Speed each to 0, to 8 and back; a Save at 4-1; Hercule City by the
world map: its talkers, the circus fighters still there after the story (3-15, 3-17); a second Save (the cleared
save); a soft reset -> title -> Continue of it; the final sequence (4-21) with every text box mashed on alternate
frames; the credits with a key held in turns (A, START, each direction, START+SELECT, A+B, L+R, ...); the
post-credits title's Options / Single Player / slot list with keys on alternate frames, ending on the title.
"""


def main():
    a = sys.argv[1:]
    st, ee, out = a[0], a[1], Path(a[2])
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 400000
    out.mkdir(parents=True, exist_ok=True)
    e = start_emu(str(Path(st).resolve()))
    sbemu.sd_write(e, Path(ee).read_bytes())
    d = PostDriver(list(GOALS))
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
                if '--trace' in a:
                    print(' ', len(log), d.k, str(d.goals[d.k] if d.k < len(d.goals) else '')[:80], s, flush=True)
    except Exception as ex:  # noqa: BLE001 - keep what ran, show where
        e.save_file(str(out / 'fail.ss'))
        e.screenshot(str(out / 'fail.png'), 2)
        (out / 'fail.eeprom').write_bytes(sbemu.sd_read(e))
        (out / 'keys.json').write_text(json.dumps({'keys': log, 'failed': True}))
        print(f'FAIL after {len(log)} frames at goal {d.k} {d.goals[d.k] if d.k < len(d.goals) else ""}: {ex}',
              status(e), flush=True)
        sys.exit(1)
    (out / 'keys.json').write_text(json.dumps({'keys': log}))
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    (out / 'end.eeprom').write_bytes(sbemu.sd_read(e))
    print(f'p3_post: {len(log)} frames {time.time() - t0:.0f}s', status(e), flush=True)


if __name__ == '__main__':
    main()
