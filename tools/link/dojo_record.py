"""dojo_record.py NAME [--frames N]: record a Dojo link fight from two linked emulators (tools/emu shim: `emu_link`,
mGBA's lockstep coordinator, one process, headless and muted).

Both games boot with run 2's in-game saves (work/run2/end.sav), go title -> Multiplayer -> Dojo, the host (emulator A,
player 0, the left fighter) picks Balanced Mode / 1 round / Start Fight, both pick the first character (Goku), then the
fight runs on SCRIPT's keys (per side). Per logic frame of A (the link game runs a logic frame every second video
frame): the frame counter, the keys each fighter's update read (0x03002DA4 / 0x03002DAE: the link's applied input,
the remote side's arrives some frames late), the generator, the round timer (0x03002BFC +4), and both fighters (A's
objects: vt 0x0804DB18 = the local one, the left; 0x0804DD94 = the remote, the right): x / y (24.8), facing, animation
raw / frame / tick, HP / EP (the fighter's own record at +0x240), the form controller's EP regen accumulator, the record's form
and Super Saiyan / SS3 gauges, the controller's slot. Writes data/link/dojo_NAME.json (no ROM data)."""
import json
import sys
import ctypes
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 're/buu', 'tools/emu', 'tools/battle/lockstep'):
    sys.path.insert(0, str(ROOT / d))
import sbemu  # noqa: E402  (the save-capable library; it has the link functions too)
from route import emu as mkemu, soft_reset  # noqa: E402
from emu import KEY  # noqa: E402
from emu_lockstep import anim_raw  # noqa: E402

L = sbemu._load_sb()
L.emu_link.argtypes = [ctypes.c_void_p, ctypes.c_int]
L.emu_link_run_frames.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int]
L.emu_link_run_frames.restype = ctypes.c_int

OBJW = 0x03001C4C
LOCAL, REMOTE = 0x0804DB18, 0x0804DD94
# title -> Multiplayer -> Dojo -> the link -> each side's saved game (the setup and the picks: `menu`)
PRE = [('-', '-', 1100), ('A', 'A', 7), ('-', '-', 40), ('DOWN', 'DOWN', 7), ('-', '-', 30), ('A', 'A', 7),
       ('-', '-', 300), ('A', 'A', 7), ('-', '-', 120), ('A', 'A', 7), ('-', '-', 120)]


def menu(setup, pick_a, pick_b):
    """the host's setup screen (`setup`: its keys, the cursor left on Start Fight - none: Balanced, 3 rounds, the
    game's defaults), A; then "Choose your character!": RIGHT `pick_a` / `pick_b` times through the roster, A"""
    m = list(PRE)
    for k in setup:
        m += [(k, '-', 7), ('-', '-', 30)]
    m += [('A', 'A', 7), ('-', '-', 120)]
    for i in range(max(pick_a, pick_b)):
        m += [('RIGHT' if i < pick_a else '-', 'RIGHT' if i < pick_b else '-', 7), ('-', '-', 30)]
    m += [('A', 'A', 7)]
    return m


# the fight's keys (video frames): walk in, punches, blocks, Energy Blasts, steps up / down, a chase
SCRIPT = [('-', '-', 200), ('RIGHT', 'LEFT', 50), ('A', '-', 4), ('-', '-', 20), ('-', 'A', 4), ('-', '-', 20),
          ('A', 'R', 4), ('A', 'R', 30), ('-', '-', 20), ('B', '-', 6), ('-', '-', 40), ('-', 'B', 6), ('-', '-', 40),
          ('UP', 'DOWN', 30), ('RIGHT', '-', 30), ('-', 'LEFT', 30), ('A', 'A', 4), ('-', '-', 10), ('A', 'A', 4),
          ('-', '-', 10), ('A', 'A', 4), ('-', '-', 30), ('LEFT', 'RIGHT', 60), ('B', 'B', 8), ('-', '-', 120),
          ('RIGHT', 'LEFT', 80), ('A', '-', 4), ('-', '-', 8), ('A', '-', 4), ('-', '-', 8), ('A', '-', 4), ('-', '-', 60)]


def brain(e, t, seed, switch=False):
    """closed-loop keys for both fighters from A's view (a fight, not a walk past): close in on the row, then a
    seeded mix of punches, blocks, Energy Blasts, side steps and waits"""
    import random
    p0, p1 = fighters(e)
    if not (p0 and p1):
        return 0, 0
    f = [fighter(e, p0), fighter(e, p1)]
    out = []
    for me, other, k in ((f[0], f[1], 0), (f[1], f[0], 1)):
        r = random.Random(seed * 1000003 + t * 2 + k)
        dx, dy = (other[0] - me[0]) >> 8, (other[1] - me[1]) >> 8
        if abs(dy) > 6:
            m = KEY.DOWN if dy > 0 else KEY.UP
        elif abs(dx) > 28:
            m = KEY.RIGHT if dx > 0 else KEY.LEFT
        else:
            m = r.choice([KEY.A, KEY.A, KEY.A, 0, 0, KEY.R, KEY.B, KEY.RIGHT if dx > 0 else KEY.LEFT,
                          KEY.LEFT if dx > 0 else KEY.RIGHT, KEY.UP, KEY.DOWN] + ([KEY.L, KEY.B] if switch else []))
        out.append(m)
    return out[0], out[1]


def keyof(s):
    m = 0
    for k in s.split('+'):
        if k and k != '-':
            m |= getattr(KEY, k)
    return m


def fighter(e, o):
    ctl = e.r32(o + 0x22C)
    # (then the record's form, Super Saiyan / SS3 gauges, and the form controller's slot)
    return [e.r32(o + 0x1A0), e.r32(o + 0x1A4), e.r8(o + 0x10), anim_raw(e, o), e.r8(o + 0x11), e.r16(o + 0x12),
            e.r16(o + 0x240 + 0x16), e.r16(o + 0x240 + 0x1A), e.r32(ctl + 0xC) if ctl else -1,
            e.r8(o + 0x240 + 0x14), e.r16(o + 0x240 + 0x1E), e.r16(o + 0x240 + 0x20), e.r32(ctl + 4) if ctl else -1]


GD = 0x03001068


def coins(e):
    n = min(e.r32(OBJW + 4), 256)
    out = []
    for k in range(n):
        o = e.r32(OBJW + 8 + 4 * k)
        if e.r32(o) == 0x08049CC4:
            # (x / y 24.8, the box's x0 / y0, age, moves)
            out.append([e.r32(o + 0x4C), e.r32(o + 0x50), e.r32(o + 0x14), e.r32(o + 0x18), e.r32(o + 0x24), e.r32(o + 0x2C)])
    return out


def fighters(e):
    n = min(e.r32(OBJW + 4), 256)
    by = {}
    for k in range(n):
        o = e.r32(OBJW + 8 + 4 * k)
        if e.r32(o) in (LOCAL, REMOTE):
            by[e.r32(o)] = o
    return by.get(LOCAL), by.get(REMOTE)


def step(a, b):
    """one video frame of A (B kept within a frame of it: a core waiting on the other sleeps)"""
    f0 = L.emu_frame(a.h)
    while L.emu_frame(a.h) == f0:
        if L.emu_link_run_frames(a.h, b.h, 1) < 0:
            raise RuntimeError('link: both emulators asleep')


TRACE = None
SHOTS = None


def main():
    global TRACE
    name = sys.argv[1]
    # (`--trace F0-F1`: the host's generator calls in those frames, by caller - the comparison's diagnostics)
    global SHOTS
    if '--shots' in sys.argv:
        k = sys.argv.index('--shots')
        SHOTS = (sys.argv[k + 1], int(sys.argv[k + 2]))
        Path(SHOTS[0]).mkdir(parents=True, exist_ok=True)
    if '--trace' in sys.argv:
        TRACE = [int(x) for x in sys.argv[sys.argv.index('--trace') + 1].split('-')]
    sav = (ROOT / 'work' / 'run2' / 'end.sav').read_bytes()
    a, b = mkemu('buu_menus.ss'), mkemu('buu_menus.ss')
    for e in (a, b):
        sbemu.sd_write(e, sav)
        e.run(3)
        soft_reset(e)
    L.emu_link(a.h, 0)
    L.emu_link(b.h, 1)
    rows = []
    recs = None
    lf_prev = None
    last, last_phase, last_k = None, None, 0
    arg = lambda k, d: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d
    # (the setup screen: the cursor starts on Start Fight; UP UP -> Character Mode's Balanced, RIGHT In-Game, A takes
    # it; DOWN -> Rounds' 1, RIGHT 3 / 5, A; DOWN -> Start Fight)
    mode, rounds = arg('--mode', 'balanced'), int(arg('--rounds', '3'))
    setup = ['UP', 'UP'] + (['RIGHT'] if mode == 'ingame' else []) + ['A', 'DOWN'] + ['RIGHT'] * [1, 3, 5].index(rounds) + ['A', 'DOWN']
    pick_a, pick_b = int(arg('--pick-a', '0')), int(arg('--pick-b', '0'))
    MENU = menu(setup, pick_a, pick_b)
    menu_len = sum(n for _, _, n in MENU)
    keys_log = []
    frames = int(sys.argv[sys.argv.index('--frames') + 1]) if '--frames' in sys.argv else 2400
    seed = int(sys.argv[sys.argv.index('--seed') + 1]) if '--seed' in sys.argv else 1
    fight = [('brain', None, frames)]
    if '--banner' in sys.argv:
        # keys from the round's banner on: does the game read them before the timer starts?
        fight = [('RIGHT', 'A', 200), ('-', '-', 60)]
    held = (0, 0, 0)
    for phase, seq in (('menu', MENU), ('fight', fight)):
        for ka, kb, n in seq:
            for t in range(n):
                if ka == 'brain':
                    # (a new decision every 6 frames, held between: presses register as presses)
                    if t % 6 == 0:
                        held = brain(a, t // 6, seed, '--switch' in sys.argv) + (0,)
                    ma, mb = (held[0], held[1]) if t % 6 < 4 else (0, 0)
                else:
                    ma, mb = keyof(ka), keyof(kb)
                a.set_keys(ma)
                b.set_keys(mb)
                step(a, b)
                keys_log.append([ma, mb])
                if SHOTS and len(keys_log) % SHOTS[1] == 0:
                    # (`--shots DIR EVERY`: both screens every EVERY video frames, for the link screens' layouts)
                    a.screenshot(Path(SHOTS[0]) / f'{len(keys_log):06d}_a.png', 1)
                    b.screenshot(Path(SHOTS[0]) / f'{len(keys_log):06d}_b.png', 1)
                lf = a.r32(0x03000F94)
                if TRACE and TRACE[0] - 2 <= lf <= TRACE[1]:
                    hs = [h for h in a.hits() if h['kind'] == 'break']
                    if hs:
                        def callers(h):
                            sp = h['regs'][13]
                            st = [a.r32(sp + 4 * i) for i in range(12)]
                            return '/'.join(hex(v & ~1) for v in st if 0x08000000 <= v < 0x08800000 and v & 1)[:60]
                        print('rng calls lf', lf, [hex(h['lr'] & ~1) + ':' + callers(h) for h in hs])
                    if not getattr(a, '_bp', False):
                        a.breakpoint(0x0803FAAC)
                        a._bp = True
                p0, p1 = fighters(a)
                if p0 and p1 and recs is None:
                    # (the fighters' records as made, obj+0x240, 0x68 bytes each: the In-Game mode's save party)
                    recs = [bytes(a.read(o + 0x240, 0x68)).hex() for o in (p0, p1)]
                snap = {'lf': lf, 'video': len(keys_log), 'keys': [a.r16(0x03002DA4), a.r16(0x03002DAE)],
                        'rng': [a.r32(0x03004A5C), a.r32(0x03004A60)], 'timer': a.r8(0x03002BFC + 4),
                        'wins': [a.r8(0x03002BFC), a.r8(0x03002BFD)],
                        # (the coins on the floor: vt 0x08049CC4; and each side's save zenie)
                        'coins': coins(a), 'zenie': [a.r32(GD + 0x4F4), b.r32(GD + 0x4F4)],
                        'p': [fighter(a, p0), fighter(a, p1)] if p0 and p1 else []}
                # a logic frame is written once the next one has begun: the last video frame it was the counter's
                # (a frame that ran past its video frame - a link stall, a heavy knockdown - completes in the next)
                if lf != lf_prev and last is not None and (last_phase == 'fight' or last['p'] or last_k > menu_len - 8):
                    rows.append(last)
                last, last_phase, last_k = snap, phase, len(keys_log)
                lf_prev = lf
    out = ROOT / 'data' / 'link' / f'dojo_{name}.json'
    out.write_text(json.dumps({'name': name, 'about': __doc__.split('\n')[0], 'mode': mode, 'rounds': rounds, 'pick': [pick_a, pick_b],
                               'video_keys': keys_log, 'recs': recs, 'rows': rows},
                              separators=(',', ':')))
    print(out, len(rows), 'logic frames')


if __name__ == '__main__':
    main()
