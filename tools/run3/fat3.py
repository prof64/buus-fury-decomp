"""Run 3's fat-Buu chapter (`f3_fat`; engine/docs/segments.md "Run 3"), recorded from the ship part's end (8-70 after
Majin Buu) with run 2's pieces (tools/run2/fat2.py: their goals and drivers, unchanged) played in a third Dragon Ball
order and with run 3's stresses between them. Input only.

Order (run 2: radar, bandana, grind_goten, airship, fukurou, pyramid, desert, pilaf, thieves, nataday, dbx): the
Earth dungeons before the ship-side ones - radar, bandana, grind_goten, desert, pilaf, thieves, airship, pyramid,
fukurou, nataday, dbx.

Stresses between the pieces (each piece starts in the field or on the world map after its predecessor): the pause
menu opened and closed as fast as it takes input (three times), every pause page both ways (in a 2D area; the
skill cursor the pages' L / R move is put back on slot 0 after), START on alternate frames for 60 frames; then B
until the field runs again.

`python3 tools/run3/fat3.py PIECE START.ss START.eeprom` plays one piece (with its stress interlude first) from a
state + cartridge save -> work/run3fat/<PIECE>.{ss,eeprom,keys.json}; `--all START.ss START.eeprom` plays them all in
order (each from the previous one's end); `--part` joins the pieces' logs into work/run3/parts/f3_fat.json."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/run2', 'tools/run3'):
    if str(ROOT / d) not in sys.path:
        sys.path.insert(0, str(ROOT / d))
import fat2  # noqa: E402  (puts the live ship driver first on the path)
from route import status  # noqa: E402

KEY = fat2.KEY


class Driver3(fat2.Driver2):
    """fat2's Driver2 with the save point's first A made closed-loop: a ('keys', [(A, ..), ..]) program right after
    ('face', 'U') (the save point menu: Save / charsel) starts with ('opensave',) - A every 40 frames until a scene
    opens, a one-pixel nudge left / right between tries (run 3's walks end a pixel off run 2's: at 10-30 the first A
    met nothing and the program's switch went to the wrong menu) - then the program without its first A"""

    def __init__(self, goals):
        out = []
        for g in goals:
            if (isinstance(g, tuple) and g and g[0] == 'keys' and out and out[-1] == ('face', 'U') and g[1]
                    and g[1][0][0] == KEY.A):
                out += [('opensave',), ('keys', list(g[1][1:]))]
                continue
            out.append(g)
        # the Bandana's equip (ship_route.equip(3) = the accessory list's first row): by the item, not the row
        import ship_route
        bandana = ship_route.equip(3)
        k = 0
        while k + 1 < len(out):
            if out[k] == bandana[0] and out[k + 1] == bandana[1]:
                out[k:k + 2] = [('equipitem', 126)]
            k += 1
        super().__init__(out)

    def step(self, e):
        g = self.goals[self.k]
        if g[0] == 'equipitem':
            # ('equipitem', item): ship_route.equip(slot, row) with the row of `item` in its slot's list counted from
            # the owned list (GameData +0xE4: count, then u32 entries item | wearer << 28; an item's slot is its ROM
            # record's +0x14, records of 0x2C from 0x086F82EC) - run 3 owns more accessories than run 2 (the trinket
            # stall), so the Bandana is not the first row
            item = g[1]
            gd = 0x03001068
            n = e.r32(gd + 0xE4)
            ents = [e.r32(gd + 0xE4 + 4 + 4 * k) & 0x0FFFFFFF for k in range(min(n, 64))]
            slot = e.r8(0x086F82EC + 0x2C * item + 0x14)
            row = 0
            for it in ents:
                if it == item:
                    break
                if e.r8(0x086F82EC + 0x2C * it + 0x14) == slot:
                    row += 1
            import ship_route
            self.goals[self.k:self.k + 1] = ship_route.equip(slot, row)
            self.t = 0
            return None
        if g[0] == 'opensave':
            s = status(e)
            if s['scene'] > 0 or s['p'] is None:
                # (the menu's first frames: wait as the program's A did - its 40-frame gap)
                self.ow = getattr(self, 'ow', 0) + 1
                if self.ow >= 40:
                    self.ow = 0
                    self.next_goal()
                    return None
                return 0
            self.ow = 0
            if self.t > 2000:
                raise RuntimeError(f'opensave: no menu {s}')
            ph = self.t % 80
            if ph < 3:
                return KEY.A
            if 40 <= ph < 41 and self.t > 80:
                return (KEY.LEFT if (self.t // 160) % 2 else KEY.RIGHT)
            if 50 <= ph < 53 and self.t > 80:
                return KEY.UP
            return 0
        return super().step(e)


fat2.Driver2 = Driver3

_real_pyramid = fat2.real_pyramid_driver


def real_pyramid_driver3():
    """run 2's RealPyramidDriver, but a fall into 14-12 (whose way back up stays shut) is not walked back: a soft
    reset (A+B+SELECT+START) - the title; the piece's Retry then Continues from 14-1's save and plays the maze again
    (an adversarial soft reset mid-dungeon besides)"""
    base = _real_pyramid()

    class Pyr3(base):
        def __init__(self, goals):
            super().__init__(goals)
            self.first = list(goals)
            self.reset_t = None

        def keys(self, e):
            if self.reset_t is not None:
                # the soft reset, then the title -> Single Player -> GAME 1 (Continue) until 14-1's field runs; then
                # this part's goals from their start (the maze again)
                self.reset_t += 1
                t = self.reset_t
                s = status(e)
                if t > 400 and s['area'][:2] == [14, 1] and s['p'] is not None and s['scene'] == 0:
                    self.reset_t = None
                    self.goals, self.k, self.t = list(self.first), 0, 0
                    return 0
                if t > 20000:
                    raise RuntimeError(f'pyramid: no Continue after the reset {s}')
                if t <= 10:
                    return KEY.A | KEY.B | KEY.SELECT | KEY.START
                ph = (t - 10) % 600
                if 300 <= ph < 303:
                    return KEY.START
                if 420 <= ph < 423 or 540 <= ph < 543:
                    return KEY.A
                return 0
            return super().keys(e)

        def step(self, e):
            g = self.goals[self.k]
            s = status(e)
            if g[0] == 'talkflag':
                # ('talkflag', dir, flag): face `dir` and A (the talk played through) until story flag `flag` is set;
                # a one-pixel nudge left / right between tries (run 3's walk ends off run 2's: man 2's s414, flag
                # 102, never started and s406 then gave no 7-Star ball)
                if e.r8(0x03001068 + 0x55 + g[2] // 8) >> (g[2] % 8) & 1 and s['scene'] == 0:
                    self.next_goal()
                    return None
                if s['scene'] > 0:
                    return self.mash_a()
                if self.t > 6000:
                    raise RuntimeError(f'talkflag: flag {g[2]} not set {s}')
                ph = self.t % 90
                d = {'U': KEY.UP, 'D': KEY.DOWN, 'L': KEY.LEFT, 'R': KEY.RIGHT}[g[1]]
                if ph < 2:
                    return d
                if 4 <= ph < 7:
                    return KEY.A
                if 60 <= ph < 61 and self.t > 90:
                    return KEY.LEFT if (self.t // 180) % 2 else KEY.RIGHT
                return 0
            # (14-12 is on the way (its up exit to 14-16) until the maze's state shuts it: stuck there 600 frames in
            # one spot = the fall's dead end)
            pos = (tuple(s['area'][:2]), tuple(s['p'][:2]) if s['p'] else None)
            if pos == getattr(self, 'p12', None):
                self.n12 += 1
            else:
                self.p12, self.n12 = pos, 0
            if s['area'][:2] == [14, 12] and self.n12 > 600:
                print('  pit fall into 14-12: soft reset -> Continue', s, flush=True)
                self.reset_t = 0
                return KEY.A | KEY.B | KEY.SELECT | KEY.START
            return super().step(e)
    return Pyr3


fat2.real_pyramid_driver = real_pyramid_driver3

_fix_db7 = fat2.fix_db7


def fix_db7_3(g):
    """fat2.fix_db7's three talks to man 2 as one closed-loop talk until flag 102 (s414) is set"""
    g = _fix_db7(g)
    k = g.index(('walk', 200, 104))
    j = g.index(('events',), k)
    return g[:k + 1] + [('talkflag', 'U', 102)] + g[j + 1:]


fat2.fix_db7 = fix_db7_3
ORDER = ['radar', 'bandana', 'grind_goten', 'desert', 'grind_trunks', 'pilaf', 'thieves', 'airship', 'pyramid',
         'fukurou', 'nataday', 'dbx']
OUT = ROOT / 'work' / 'run3fat'


def menu_open(e):
    s = status(e)
    return s['p'] is None and s['scene'] == 0


class Interlude:
    """the stresses between pieces, closed-loop on the emulator state: a field with the leader first (B until then),
    three pause spins (START, wait for the menu, START, wait for the field), the pause pages R x5 / L x5, START on
    alternate frames for 60 frames, B until the field runs for 30 frames"""

    def __init__(self):
        self.ph = 'settle'
        self.t = 0
        self.n = 0
        self.prog = None
        self.k = 0
        self.goals = ['interlude']
        self.idle = 0

    def done(self):
        return self.ph == 'done'

    def field(self, e):
        s = status(e)
        return s['p'] is not None and s['scene'] == 0

    def keys(self, e):
        self.t += 1
        if self.t > 20000:
            raise RuntimeError(f'interlude stuck in {self.ph} {status(e)}')
        if self.ph == 'settle':
            if self.field(e):
                self.idle += 1
                if self.idle >= 30:
                    self.ph, self.idle, self.sub = 'spin', 0, 0
                    # (the pause pages' L / R also move the skill cursor: the piece gets slot 0 - the Energy Blast its
                    # goals expect, e.g. db2's rocks - after)
                    self.slot = 0
                return 0
            self.idle = 0
            return KEY.B if self.t % 30 < 3 else 0
        if self.ph == 'spin':
            if self.n >= 3:
                # the pause pages both ways (R / L in the menu) only in a 2D area: there they also move the skill
                # cursor, which the 'slot' phase puts back; on the world map the flyer has no skill to restore
                self.prog = []
                if status(e)['area'][:2] != [0, 50]:
                    self.prog = [KEY.START] + [0] * 40
                    for k in [KEY.R] * 5 + [KEY.L] * 5:
                        self.prog += [k] + [0] * 24
                    self.prog += [KEY.B] + [0] * 30
                self.ph = 'tour'
                return 0
            up = menu_open(e)
            self.sub += 1
            st = getattr(self, 'st', 'open')
            if st == 'open':
                if up:
                    self.st, self.sub = 'close', 0
                    return 0
                if self.sub > 240:
                    # (the pause does not open here: the refused presses were the stress)
                    self.n, self.sub = 3, 0
                    return 0
                return KEY.START if self.sub % 40 == 1 else 0
            # close as soon as the menu takes input (START every 2 frames until the field is back)
            if not up:
                self.st, self.sub = 'open', 0
                self.n += 1
                return 0
            if self.sub > 600:
                raise RuntimeError(f'interlude: the pause does not close {status(e)}')
            return KEY.START if self.sub % 2 == 1 else 0
        if self.ph == 'tour':
            if self.prog:
                return self.prog.pop(0)
            self.ph, self.t2 = 'mash', 0
            return 0
        if self.ph == 'mash':
            self.t2 += 1
            if self.t2 > 60:
                self.ph, self.idle = 'out', 0
                return 0
            return KEY.START if self.t2 % 2 == 0 else 0
        if self.ph == 'out':
            if self.field(e):
                self.idle += 1
                if self.idle >= 30:
                    self.ph, self.t3 = ('slot' if status(e)['area'][:2] != [0, 50] else 'done'), 0
                return 0
            self.idle = 0
            return KEY.B if self.t % 30 < 3 else 0
        if self.ph == 'slot':
            # L (2 frames on, 12 off) until the skill slot the piece started with is selected again
            self.t3 += 1
            if e.r8(0x03001560 + 0x68 * e.r8(0x03001068) + 0x15) == self.slot and self.t3 % 14 == 0:
                self.ph = 'done'
                return 0
            if self.t3 > 600:
                raise RuntimeError(f'interlude: skill slot {self.slot} not reached {status(e)}')
            return KEY.L if self.t3 % 14 < 2 else 0
        return 0


def bandana3(e):
    """run 2's bandana piece with its hunt made safe to lose: Trunks, the Z-Mart food, then a Save at West City 4-1's
    save point and the world-map hunt for the Bandana (126) under a Retry - each game over (Trunks dies to the
    encounters) continues from that save and hunts again (run 2's own hunt also let him die; its stored piece was a
    lucky search)"""
    g = fat2.g_bandana(e)
    k = g.index(('hunt', 126))
    pre = g[:k - 5]          # up to the take-off walk (take_off = 5 goals)
    hunt = g[k - 5:]
    return fat2.Chain([
        lambda e: fat2.Driver2(pre + fat2.save_at(4, 1)),
        lambda e: fat2.Retry([lambda e: fat2.Driver2(list(hunt))],
                             [fat2.DEATH_CONTINUE, ('until', 4, 1), ('wait', 30)], tries=25),
    ])


def grind_trunks3(e):
    """Pilaf's Castle's gate s517 wants Trunks at level 80 (run 2 got him there in the ship-side dungeons it played
    first): Southwest Forest 8-1's save point, Trunks, his levels to 80 in 8-6 / 8-3, Goten again, the pad"""
    return fat2.Driver2([
        ('wait', 60), fat2.SW_FOREST, ('wait', 30),
        fat2.nav(8, 1, False), ('unstick',), ('walk', 148, 186), *fat2.charsel(1),
        ('grind', 80, [(8, 6), (8, 3)]),
        fat2.nav(8, 1, False), ('unstick',), ('walk', 148, 186), *fat2.charsel(-1),
        fat2.nav(8, 1, False), ('unstick',), *fat2.take_off(280, 266),
    ])


def driver3(name, e):
    if name == 'bandana':
        return bandana3(e)
    if name == 'grind_trunks':
        return grind_trunks3(e)
    return fat2.driver(name, e)


def play(drv, e, keys, cap, label):
    prev = None
    while not drv.done():
        if len(keys) >= cap:
            raise RuntimeError(f'{label}: cap')
        k = drv.keys(e) or 0
        e.set_keys(k)
        e.run(1)
        keys.append(k)
        s = status(e)
        if (s['area'][:2], drv.k) != prev:
            prev = (s['area'][:2], drv.k)
            g = drv.goals[drv.k] if not drv.done() and drv.k < len(drv.goals) else None
            print(label, len(keys), s, 'goal', drv.k, str(g)[:70], flush=True)


def one(name, start, eeprom, cap=400000):
    import sbemu
    from route import start_emu
    OUT.mkdir(parents=True, exist_ok=True)
    e = start_emu(str(Path(start).resolve()))
    sbemu.sd_write(e, Path(eeprom).read_bytes())
    keys = []
    try:
        play(Interlude(), e, keys, cap, f'{name}/interlude')
        play(driver3(name, e), e, keys, cap, name)
    except Exception as ex:  # noqa: BLE001
        e.save_file(str(OUT / f'{name}.fail.ss'))
        (OUT / f'{name}.fail.eeprom').write_bytes(sbemu.sd_read(e))
        e.screenshot(str(OUT / f'{name}.fail.png'), 2)
        (OUT / f'{name}.fail.keys.json').write_text(json.dumps(keys))
        print('FAIL', name, len(keys), ex, status(e), flush=True)
        return False
    e.set_keys(0)
    e.save_file(str(OUT / f'{name}.ss'))
    (OUT / f'{name}.eeprom').write_bytes(sbemu.sd_read(e))
    (OUT / f'{name}.keys.json').write_text(json.dumps(keys))
    print('done', name, len(keys), status(e), flush=True)
    return True


def main():
    a = sys.argv[1:]
    if a[0] == '--part':
        keys, sizes = [], {}
        for p in ORDER:
            k = json.loads((OUT / f'{p}.keys.json').read_text())
            keys += k
            sizes[p] = len(k)
        (ROOT / 'work' / 'run3' / 'parts' / 'f3_fat.json').write_text(json.dumps(keys))
        print('f3_fat', len(keys), sizes)
        return
    if a[0] == '--all':
        st, ee = a[1], a[2]
        i0 = ORDER.index(a[a.index('--from') + 1]) if '--from' in a else 0
        for name in ORDER[i0:]:
            if not one(name, st, ee):
                sys.exit(1)
            st, ee = str(OUT / f'{name}.ss'), str(OUT / f'{name}.eeprom')
        return
    if not one(a[0], a[1], a[2]):
        sys.exit(1)


if __name__ == '__main__':
    main()
