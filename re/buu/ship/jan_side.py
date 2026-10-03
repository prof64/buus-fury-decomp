"""RealSide: tools/side/hfil.py's SideDriver for the REAL route's Goku (imported by jan_hfil.py)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'side'))
import hfil  # noqa: E402
import shipdrv  # noqa: E402


class RealSide(hfil.SideDriver):
    """the Annihilator (type 1, 18-6) breathes fire with the Destroyer's states (0x82 .. 0x86): the flame guard
    for it too (the real Goku, level 120, is burnt down from 1267 HP to 80 standing in it)"""

    def flame_guard(self, e, s):
        orig = shipdrv.enemies
        shipdrv.enemies = lambda ee: [o[:5] + (20,) + o[6:] if o[5] == 1 else o for o in orig(ee)]
        try:
            return super().flame_guard(e, s)
        finally:
            shipdrv.enemies = orig


    GHOSTLY = (0x5d,)       # Poltergeist: the Ghost's AI (touchable in states 0xD9 .. 0xDB only)

    def ghost_guard(self, e, s):
        """a visible Poltergeist within reach of a step: punch it (two hits at level 120) before it and its
        invisible mates drain Goku during the maze's Instant Transmission work"""
        x, y, pf = s['p']
        K = hfil.KEY
        for o in shipdrv.enemies(e):
            if o[5] not in self.GHOSTLY:
                continue
            st = shipdrv.ai_state(e, o[0])
            if not st or st[-1] not in (0xD9, 0xDA, 0xDB):
                continue
            dx, dy = o[1] - x, o[2] - y
            if abs(dy) <= 12 and abs(dx) <= 44:
                if abs(dx) < 3:
                    return K.LEFT
                if abs(dx) > 21:
                    return K.RIGHT if dx > 0 else K.LEFT
                face, turn = (3, K.RIGHT) if dx > 0 else (2, K.LEFT)
            elif abs(dx) <= 10 and abs(dy) <= 44:
                if abs(dy) > 26:
                    return K.DOWN if dy > 0 else K.UP
                if abs(dy) < 8:
                    return K.LEFT
                face, turn = (0, K.DOWN) if dy > 0 else (1, K.UP)
            else:
                continue
            if pf != face:
                return turn
            return K.A if self.t % 6 < 2 else 0
        return None

    def step(self, e):
        s = hfil.status(e)
        if s['scene'] == 0 and s['p'] is not None and s['area'][:2] == [18, 10]:
            k = self.ghost_guard(e, s)
            if k is not None:
                return k
        return super().step(e)


hfil.SideDriver = RealSide
