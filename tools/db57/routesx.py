"""After the seventh Dragon Ball (start state `dbx`: Goten at Nataday Village's save point, all seven balls held):
15-1's pad -> the world map -> the airship marker (enemy type 141 placed at (736, 1072), on-defeat s017: Bulma takes
the balls, Shenron, s215 .. s217 on Kami's Lookout: now Goku) -> 6-1's pad -> the world map circle at (1353, 1145)
(s026, flag 119 set / 120 clear; touched at altitude 44 .. 76) -> 0-21: Goku vs Majin Buu -> s027 (flag 120) -> s218
(the fusion training, Gotenks vs Buu)."""
import math

from dbdriver import DbDriver, KEY, enemies, status

A, B, U, D, L, R = KEY.A, KEY.B, KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT


def sv(v):
    return v - (1 << 32) if v & 0x80000000 else v


class XDriver(DbDriver):
    AVOID = (20, 64, 25, 4, 29)      # Babidi hovers over 0-21 and takes no hits: never a target

    def flyer(self, e):
        p = e.r32(0x03001C34)
        if not p:
            return None
        return (sv(e.r32(p + 0x1A0)) / 256, sv(e.r32(p + 0x1A4)) / 256, sv(e.r32(p + 0x1A8)) / 1024,
                sv(e.r32(p + 0x1AC)) / 1024)

    def steer(self, x, y, h, tx, ty):
        want = math.degrees(math.atan2(tx - x, -(ty - y))) * 256 / 360
        diff = (want - h + 128) % 256 - 128
        k = 0
        if diff > 3:
            k |= R
        elif diff < -3:
            k |= L
        if abs(diff) < 40:
            k |= U
        return k

    def watch_this(self, e):
        """Watch This! (mode object vt 0x0804A3E8, its game object at +4: the icon sequence at +0x24, the state
        function at +0x30 = 0x08027FB9 once it takes input): press the sequence, 2 frames each, 8 apart, from 2 frames
        after the input state starts; None outside the mode"""
        m = e.r32(0x03000F90)
        if e.r32(m) != 0x0804A3E8:
            self._w_at = None
            return None
        gobj = e.r32(m + 4)
        self._wt = getattr(self, '_wt', 0) + 1
        if e.r32(gobj + 0x30) != 0x08027FB9:
            self._w_at = None
            return 0
        if getattr(self, '_w_at', None) is None:
            self._w_at = self._wt
            self._w_seq = [e.r8(gobj + 0x24 + k) for k in range(5)]
        i, r = divmod(self._wt - self._w_at - 2, 8)
        if 0 <= i < 5 and r < 2:
            return [1, 2, 16, 32, 64, 128][self._w_seq[i]]
        return 0

    def keys(self, e):
        k = self.watch_this(e)
        if k is not None:
            return k
        return super().keys(e)

    def step(self, e):
        g = self.goals[self.k]
        kind = g[0]
        s = status(e)
        if kind == 'flyinto':
            # ('flyinto', x, y): on the world map, fly into the type-141 marker placed nearest to (x, y): cruise
            # high, come down on it; done when its event starts
            if s['scene'] > 0 or s['area'][:2] != [0, 50]:
                self.next_goal()
                return None
            f = self.flyer(e)
            if f is None:
                return 0
            x, y, z, h = f
            if getattr(self, 'mark', None) is None:
                ms = [o for o in enemies(e) if o[5] == 141]
                if not ms:
                    return 0
                self.mark = min(ms, key=lambda o: math.hypot(o[1] - g[1], o[2] - g[2]))[0]
            o = next((o for o in enemies(e) if o[0] == self.mark), None)
            if o is None:
                return 0
            dist = math.hypot(o[1] - x, o[2] - y)
            k = self.steer(x, y, h, o[1], o[2])
            if dist > 90 and z < 100:
                k |= B
            elif dist <= 90 and z > 2:
                k |= A
            return k
        if kind == 'flyalt':
            # ('flyalt', x, y, w, h, zlo, zhi): fly into the rect at an altitude inside (zlo, zhi) (a touch_altitude
            # trigger); done when an event starts / the area changes
            if s['scene'] > 0 or s['area'][:2] != [0, 50]:
                self.next_goal()
                return None
            f = self.flyer(e)
            if f is None:
                return 0
            x, y, z, h = f
            tx, ty = g[1] + g[3] / 2, g[2] + g[4] / 2
            mid = (g[5] + g[6]) / 2
            k = self.steer(x, y, h, tx, ty)
            if z < mid - 4:
                k |= B
            elif z > mid + 4:
                k |= A
            return k
        if kind == 'storyto':
            # ('storyto', w, a[, 'enter']): ShipDriver's 'story' (events mashed, enemies fought, bosses not skipped), done 20
            # frames after the player is controllable in area w-a whatever lives there (the world map's patrols)
            if s['area'][:2] == [g[1], g[2]] and len(g) > 3 and g[3] == 'enter':
                self.next_goal()
                return None
            if s['area'][:2] == [g[1], g[2]] and s['scene'] == 0 and s['p'] is not None:
                self.idle += 1
                if self.idle >= 20:
                    self.next_goal()
                    return None
                return 0
            if s['scene'] > 0 or s['p'] is None or s['area'][:2] == [0, 50]:
                self.idle = 0
                return self.mash_a()
            if enemies(e):
                if any(o[5] in self.BUU for o in enemies(e)):
                    return self.buu(e, s)
                self.in_nav = True
                self.fight_done = False
                self.idle = 29          # fightp reports "no target" (fight_done) at once
                k = self.fightp(e, s, ('fightp', 10 ** 9, 99999, 6, True))
                self.in_nav = False
                self.idle = 0
                if self.fight_done:
                    # nobody the walk grid reaches (Majin Buu against 0-21's wall, stretch-punching from there): walk
                    # straight at the nearest fighter's side
                    x, y = s['p'][0], s['p'][1]
                    o = min((o for o in enemies(e) if o[5] not in (25, 4, 29)), default=None,
                            key=lambda o: abs(o[1] - x) + abs(o[2] - y))
                    if o is not None:
                        tx, ty = o[1] + (16 if x >= o[1] else -16), o[2]
                        k = 0
                        if abs(x - tx) > 1:
                            k |= R if tx > x else L
                        if abs(y - ty) > 1:
                            k |= D if ty > y else U
                        if not k:
                            self.mash += 1
                            k = A if self.mash % 6 < 2 else 0
                        # blocked (Babidi's footprint between the two): 28 frames around it, below then above
                        st = getattr(self, '_fb', [None, 0, 0, 0])
                        if st[2] > 0:
                            st[2] -= 1
                            k = (D if st[3] % 2 == 0 else U) | (k & (L | R) if st[2] < 14 else 0)
                        elif st[0] == (x, y) and k & (L | R | U | D):
                            st[1] += 1
                            if st[1] > 8:
                                st[1], st[2], st[3] = 0, 28, st[3] + 1
                        else:
                            st[1] = 0
                        st[0] = (x, y)
                        self._fb = st
                return k
            self.idle = 0
            return self.mash_a()
        return super().step(e)


def dbx():
    return [
        ('mark', 'x00_start'),
        ('go', 15, 33, False), ('go', 15, 1, False),       # in and out of the elder's hut: an area-entry cut
        ('trig', 384, 628, A), ('wait', 60),               # 15-1's pad: the world map
        ('mark', 'x01_map'),
        ('flyinto', 736, 1072),                            # the airship marker: s017
        ('mark', 'x02_s017'),
        ('story', 6, 1, True),                             # s017 -> s215 -> s216 -> s217: Goku on Kami's Lookout
        ('mark', 'x03_goku'),
        ('go', 6, 2, False), ('go', 6, 1, False),          # into the Lookout's building and back: an area-entry cut
        ('trig', 304, 556, A), ('wait', 60),               # 6-1's pad: the world map
        ('mark', 'x04_map'),
        ('flyalt', 1353, 1145, 63, 55, 44, 76),            # s026 -> 0-21
        ('mark', 'x05_s026'),
        ('storyto', 0, 50),                                # Goku vs Majin Buu, s027, s218 (the fusion): Gotenks
        ('mark', 'x06_gotenks'),
        ('flyalt', 1121, 1217, 63, 55, 44, 76),            # s020 -> 0-15: Gotenks vs Buu
        ('mark', 'x07_s020'),
        ('storyto', 0, 96, 'enter'),                       # the fight, back on the Lookout; ends where the Janemba
        ('mark', 'x08_end'),                               # saga's prologue starts (0-96 -> 3-4 -> 0-16: side content)
    ]


ROUTES = {'dbx': dbx}
DRIVER = XDriver
