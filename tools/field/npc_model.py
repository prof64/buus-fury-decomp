"""npc_model.py [NAME ...]: replay data/field/npc_<name>.json with a Python model of the NPC behaviours and report the
first divergence (position, facing, raw anim, anim frame, RNG). The model is the spec handed to the Rust port.

Inputs besides the trace: the live walk / enemy grids at frame 0 (read from the emulator after the same warp;
compared with the content collision + the NPC marks to check the mark rule) and the player's footprint per frame
(from the trace; the player updates before the NPCs in the object list, so an NPC sees the player's new position).
Animation: SetAnim(0) idle state (enter: tick = rand(255), frame 0; tick: tick+1, 2-frame anims blink
(frame 1 while (tick & 255) > 240), else frame = tick >> 3, wrapping to frame 0 / tick 0 at nframes);
SetAnim(1) walk state (enter: tick 0 frame 0; tick: tick+1, frame = tick >> 3, wrap to tick 0 / frame 0).
nframes per sprite / raw anim come from content sprites (work/field_content is maps only, so nframes are read
from the emulator: def + 0x50 + 4 raw -> anim struct, word 0)."""
import json
import sys
from npc_lib import *
import rec_npc

GRID = 256
MAPOBJ = 0x030019A4


class Rng:
    def __init__(self, s0, s1):
        self.s0, self.s1 = s0, s1

    def next(self):
        a = ((self.s0 + 0x02CBBC71) & 0xFFFFFFFF) ^ self.s1
        rol = ((self.s1 << 3) | (self.s1 >> 29)) & 0xFFFFFFFF
        b = ((rol + 0x0632D80F) & 0xFFFFFFFF) ^ a
        self.s0, self.s1 = a, b
        return (a - b) & 0xFFFFFFFF

    def rand(self, n):
        return ((self.next() & 0xFFFF) * n) >> 16

    def chance(self, p):
        return (self.next() & 0xFFFF) < p


class Grids:
    def __init__(self, walk, enemy):
        self.g = {'walk': bytearray(walk), 'enemy': bytearray(enemy)}

    def span(self, r):
        x0, y0, x1, y1 = r
        return (max(x0 >> 3, 0), max(y0 >> 3, 0), min(256 if x1 >= 2048 else (x1 + 7) >> 3, GRID),
                min(256 if y1 >= 2048 else (y1 + 7) >> 3, GRID))

    def mark(self, r, v, layers=('walk', 'enemy')):
        """0x08009930(rect, grid, value)"""
        tx0, ty0, tx1, ty1 = self.span(r)
        for l in layers:
            g = self.g[l]
            for ty in range(ty0, ty1):
                for tx in range(tx0, tx1):
                    g[ty * GRID + tx] = v

    def test(self, r, layer='walk'):
        """0x08009324(rect, grid): all tiles under [x0, x1) x [y0, y1) passable and inside 0..2048"""
        x0, y0, x1, y1 = r
        if x0 < 0 or y0 < 0 or x1 > 2048 or y1 > 2048:
            return False
        g = self.g[layer]
        for ty in range(y0 >> 3, (y1 + 7) >> 3):
            for tx in range(x0 >> 3, (x1 + 7) >> 3):
                if tx >= GRID or ty >= GRID or g[ty * GRID + tx] != 1:
                    return False
        return True


def overlap(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


class Npc:
    def __init__(self, info, steps, nframes, talk):
        self.x, self.y, self.facing = info['x'], info['y'], info['facing']
        self.w, self.h = info['foot']
        self.talk = talk
        self.steps = steps          # [(fn, words)]
        self.idx = -1
        self.cur = None
        self.nframes = nframes      # raw -> nframes
        self.anim = 0               # SetAnim id (0 idle, 1 walk)
        self.raw = info['raw']
        self.frame = info['anim_frame']
        self.tick = None            # unknown at frame 0 (the ctor's rand(255) happened during the load)

    def foot(self):
        return (self.x >> 8, self.y >> 8, (self.x >> 8) + self.w, (self.y >> 8) + self.h)


class Sim:
    def __init__(self, trace, grids, nframes_of, load_ticks):
        self.t = trace
        i = trace['init']
        self.rng = Rng(*i['rng_f0'])
        self.G = grids
        self.npcs = []
        for k, n in enumerate(i['npcs']):
            d = int(n['def'], 16)
            ctor = u32(d)
            off = 0x14 if ctor == 0x08017DA3 else 0x10
            cnt = u32(d + off)
            steps = []
            for j in range(cnt):
                rec = u32(d + off + 4 + 4 * j)
                steps.append((u32(rec), rec))
            npc = Npc(n, steps, nframes_of[k], ctor == 0x08017DA3)
            npc.tick = load_ticks.get(n['obj'])
            self.npcs.append(npc)
        self.player = None

    # --- animation (state singletons 0x03001B0C idle / 0x03001B58 walk)
    def set_anim(self, n, a, force=True):
        if not force and n.anim == a:
            return
        n.anim = a
        n.raw = a + 1
        if a == 0:
            n.tick = self.rng.rand(255)
        else:
            n.tick = 0
        n.frame = 0

    def anim_tick(self, n):
        nf = n.nframes.get(n.raw, 1)
        n.tick = (n.tick + 1) & 0xFFFF
        if n.anim == 0 and nf == 2:
            n.frame = 1 if (n.tick & 0xFF) > 0xF0 else 0
            return
        f = (n.tick >> 3) & 0xFF
        if f >= nf:
            n.frame = 0
            n.tick = 0
        else:
            n.frame = f

    # --- steps
    def create(self, n, fn, rec):
        s = {'fn': fn}
        if fn == 0x0800A26F:
            s['kind'] = 'stand'
        elif fn == 0x0800A223:
            self.set_anim(n, 0, force=False)
            n.facing = u8(rec + 6)
            s.update(kind='wait', count=s16(rec + 4))
        elif fn in (0x0800A8E9, 0x0800A90F):
            self.set_anim(n, 0 if fn == 0x0800A8E9 else 1)
            s.update(kind='face_player', count=s32(rec + 4))
        elif fn in (0x0800A1AF, 0x0800A1D5):
            if fn == 0x0800A1D5:
                raise NotImplementedError('enemy walk_to')
            tx, ty = s16(rec + 4) << 8, s16(rec + 6) << 8
            dx, dy = tx - n.x, ty - n.y
            adx, ady = abs(dx), abs(dy)
            if adx >= ady:
                d, major = (3 if dx >= 0 else 2), adx
            else:
                d, major = (0 if dy >= 0 else 1), ady
            spd = u16(rec + 8)
            cnt = ((major << 8) // spd >> 8) & 0xFFFF
            n.facing = d
            if cnt:
                vx, vy = tdiv(dx << 8, cnt << 8), tdiv(dy << 8, cnt << 8)
            else:
                vx = vy = 0
                cnt = 1
            s.update(kind='walk_to', tx=s16(rec + 4), ty=s16(rec + 6), dir=d, n=cnt, vx=vx, vy=vy)
        elif fn in (0x0800A75B, 0x0800A7A1, 0x0800A7DF, 0x0800A811):
            self.set_anim(n, 0)
            spd = u32(rec + 4)
            r = ((spd << 5) + 0x7FF) & ~0x7FF
            s.update(kind='wander', spd=spd, dist=r >> 8, frames=(r + spd - 1) // spd, left=0, chance=spd << 4,
                     talk=fn in (0x0800A75B, 0x0800A7A1), rect=None)
            if fn in (0x0800A7A1, 0x0800A811):
                s['rect'] = (u16(rec + 8), u16(rec + 10), u16(rec + 12), u16(rec + 14))
            if fn == 0x0800A811:
                s['chance'] <<= 3
        else:
            raise NotImplementedError(hex(fn))
        return s

    def update_step(self, n, s):
        k = s['kind']
        if k == 'stand':
            return False
        if k == 'wait':
            s['count'] -= 1
            return s['count'] < 0
        if k == 'face_player':
            if self.player:
                dx = self.player[0] - n.x
                dy = self.player[1] - n.y
                if abs(dx) >= abs(dy):
                    n.facing = 3 if dx > 0 else 2
                else:
                    n.facing = 0 if dy > 0 else 1
            s['count'] -= 1
            return s['count'] < 0
        if k == 'walk_to':
            return self.walk_to(n, s)
        if k == 'wander':
            self.wander(n, s)
            return False

    def walk_to(self, n, s):
        old = n.foot()
        n.x += s['vx']
        n.y += s['vy']
        new = n.foot()
        if self.player:
            p = self.pfoot()
            pa = ((p[0] >> 3) << 3, (p[1] >> 3) << 3, ((p[2] + 7) >> 3) << 3, ((p[3] + 7) >> 3) << 3)
            if overlap(pa, old) or overlap(pa, new):
                n.x -= s['vx']
                n.y -= s['vy']
                return False
        self.G.mark(old, 1)
        if not self.G.test(new):
            self.G.mark(old, 0)
            self.set_anim(n, 0, force=False)
            n.x -= s['vx']
            n.y -= s['vy']
            return False
        self.set_anim(n, 1, force=False)
        n.facing = s['dir']
        self.G.mark(new, 0)
        s['n'] = (s['n'] - 1) & 0xFFFF
        if s['n'] == 0:
            self.G.mark(n.foot(), 1)
            n.x, n.y = s['tx'] << 8, s['ty'] << 8
            self.G.mark(n.foot(), 0)
            return True
        return False

    def pfoot(self):
        x, y = self.player[0] >> 8, self.player[1] >> 8
        return (x, y, x + 16, y + 8)

    def wander_check(self, n, s):
        new = s['new']
        if s['rect']:
            r = s['rect']
            if r[0] > new[0] or r[1] > new[1] or r[2] < new[2] or r[3] < new[3]:
                return False
        if not s['talk']:
            return self.G.test(new)
        if self.player and overlap(self.pfoot(), new):
            return False
        self.G.mark(s['old'], 1)
        ok = self.G.test(new)
        self.G.mark(new if ok else s['old'], 0)
        return ok

    def wander(self, n, s):
        if s['left'] == 0:
            if self.rng.chance(s['chance']):
                s['old'] = n.foot()
                d = self.rng.rand(5)
                if d < 4:
                    n.facing = d
                sx, sy = n.x >> 8, n.y >> 8
                sp, ds = s['spd'], s['dist']
                v = {0: (0, sp, 0, ds), 1: (0, -sp, 0, -ds), 2: (-sp, 0, -ds, 0), 3: (sp, 0, ds, 0)}[n.facing]
                s['vx'], s['vy'] = v[0], v[1]
                s['tx'], s['ty'] = sx + v[2], sy + v[3]
                o = s['old']
                s['new'] = (o[0] + v[2], o[1] + v[3], o[2] + v[2], o[3] + v[3])
                if self.wander_check(n, s):
                    s['left'] = s['frames']
                    s['dir'] = n.facing
                    self.set_anim(n, 1, force=False)
        if s['left'] != 0:
            n.x += s['vx']
            n.y += s['vy']
            n.facing = s['dir']
            s['left'] -= 1
            if s['left'] == 0:
                n.x, n.y = s['tx'] << 8, s['ty'] << 8
                if s['talk']:
                    self.G.mark(s['new'], 1)
                    self.G.mark(n.foot(), 0)
                self.set_anim(n, 0, force=False)
            else:
                self.set_anim(n, 1, force=False)

    def runner(self, n):
        while True:
            if n.cur is not None:
                if not self.update_step(n, n.cur):
                    return
            n.idx += 1
            if n.idx >= len(n.steps):
                n.idx = 0
            fn, rec = n.steps[n.idx]
            n.cur = self.create(n, fn, rec)

    modal = None
    MODAL_TICK_START = False  # does the talk-start frame tick the NPC animations?

    def modal_step(self, f, row):
        """a talk event runs (nested run_mode loop entered from the player's A press): behaviour runners stop,
        animations keep ticking"""
        self.player = row['p']
        if self.modal is None:
            self.modal = row['talk_npc']
            k = [n['obj'] for n in self.t['init']['npcs']].index(row['talk_npc'])
            pred = self.talk_target()
            print(f'   talk at frame {f}: emu npc {k}, predicted {pred}')
            n = self.npcs[k]
            n.facing = self.player[2] ^ 1
            self.set_anim(n, 0)
            # the event start (0x0801246E) broadcasts message 0xF to every object (last to first): an actor in the
            # walk state goes idle (walk state vt+0x10 0x0800E050 = SetAnim(0))
            for m in reversed(self.npcs):
                if m.anim == 1:
                    self.set_anim(m, 0)
            if not self.MODAL_TICK_START:
                return
        for n in reversed(self.npcs):
            self.anim_tick(n)

    TALK_BOX = {0: (0, 0, 16, 24), 1: (0, -16, 16, 8), 2: (-16, -8, 16, 8), 3: (0, -8, 32, 8)}

    def talk_target(self):
        """0x08012A24 / 0x08012B2C: message 3 with box = player (x, y) px + TALK_BOX[facing], sent to the objects last to
        first, the first talk NPC (with an event) whose footprint overlaps (strictly) accepts"""
        px, py, fc = self.player[0] >> 8, self.player[1] >> 8, self.player[2]
        b = self.TALK_BOX[fc]
        box = (px + b[0], py + b[1], px + b[2], py + b[3])
        for k in reversed(range(len(self.npcs))):
            n = self.npcs[k]
            if n.talk and self.t['init']['npcs'][k]['talk_event'] and overlap(n.foot(), box):
                return k
        return None

    def external(self, d):
        """replay one RNG call made by something other than the NPCs (the player)"""
        if d[1] == 'rand':
            self.rng.rand(d[2])
        elif d[1] == 'chance':
            self.rng.chance(d[2])
        else:
            self.rng.next()

    def step(self, f):
        row = self.t['frames'][f]
        self.player = row['p']
        npc_objs = {n['obj'] for n in self.t['init']['npcs']}
        mine = [i for i, d in enumerate(row['draws']) if len(d) > 3 and d[3] in npc_objs]
        ext = [(i, d) for i, d in enumerate(row['draws']) if not (len(d) > 3 and d[3] in npc_objs)]
        first = mine[0] if mine else len(row['draws'])
        for i, d in ext:
            if i < first:
                self.external(d)
        # update order: last to first in the object list (the NPCs follow the area's def order)
        for n in reversed(self.npcs):
            self.runner(n)
            self.anim_tick(n)
        for i, d in ext:
            if i >= first:
                self.external(d)


def live_grid(e, off):
    """the game's grids are bitmaps: 32 bytes per row of 256 tiles, bit (tx & 7) of byte ty * 32 + (tx >> 3)"""
    raw = e.read(e.r32(MAPOBJ + off), 32 * GRID)
    return bytes((raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1 for ty in range(GRID) for tx in range(GRID))


def nframes_table(e, objs):
    out = []
    for o in objs:
        d = e.r32(o + 0x58)
        t = {}
        first = {}
        for raw in range(0, 24):
            a = e.r32(d + 0x50 + 4 * raw)
            if 0x08000000 <= a < 0x0A000000:
                t[raw] = u32(a)
                first.setdefault(a, raw)
                t[('canon', raw)] = first[a]
        out.append(t)
    return out


def run(name, verbose=True):
    t = json.load(open(ROOT / 'data' / 'field' / f'npc_{name}.json'))
    scn = next(s for s in rec_npc.SCENARIOS if s['name'] == name)
    e, _, _, _ = rec_npc.goto_f0(scn)
    walk = live_grid(e, 0xA8)
    enemy = live_grid(e, 0xB0)
    objs = [int(n['obj'], 16) for n in t['init']['npcs']]
    nfr = nframes_table(e, objs)
    ticks = {hex(o): e.r16(o + 0x12) for o in objs}
    sim = Sim(t, Grids(walk, enemy), nfr, ticks)
    first = None
    for f, row in enumerate(t['frames']):
        if not row['logic']:
            continue
        if row.get('talk_npc'):
            sim.modal_step(f, row)
        else:
            if sim.modal is not None:
                # the event ended inside this frame's nested loop: its animation pass ran, then run_mode returned
                # into the player's update and the rest of the object list updates normally
                for n in reversed(sim.npcs):
                    sim.anim_tick(n)
            sim.modal = None
            sim.step(f)
        got = [[n.x, n.y, n.facing, n.nframes.get(('canon', n.raw), n.raw), n.frame] for n in sim.npcs]
        want = [r[:5] if r else None for r in row['npcs']]
        rng_ok = [sim.rng.s0, sim.rng.s1] == row['rng']
        if got != want or not rng_ok:
            first = f
            if verbose:
                print(f'{name}: first divergence at frame {f} (rng {"ok" if rng_ok else "DIFF"})')
                for k, (g, w_) in enumerate(zip(got, want)):
                    if g != w_:
                        print('   npc', k, t['init']['npcs'][k]['def'], 'sim', g, 'emu', w_)
                print('   draws', row['draws'])
            break
    if first is None and verbose:
        print(f'{name}: all {len(t["frames"])} frames match')
    return first


if __name__ == '__main__':
    for n in sys.argv[1:] or [s['name'] for s in rec_npc.SCENARIOS]:
        run(n)
