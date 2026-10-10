"""Routes (dbdriver goals) for the Dragon Ball dungeons 5-7 and the events after the seventh ball."""
from dbdriver import KEY

A, B, U, D, L, R = KEY.A, KEY.B, KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT


def db5():
    """Airship XJ-5 (world 12) from the 12-1 save point: the decks 12-1 .. 12-3, the lift into the interior, 12-4 ..
    12-27 (rooms cleared on the way in), the lift to the Control Room 12-28 (s386), the Warlord, the 5-Star Dragon
    Ball (s387: 3-minute self destruct), back out to 12-1's pad without fighting, the world map."""
    return [
        ('mark', 'a00_start'),
        ('go', 12, 27, 'near'),
        ('mark', 'a01_12-27'),
        ('trig', 104, 356, U),                    # the lift: s386 (Warlord)
        ('mark', 'a02_warlord'),
        ('clear', 20000),
        ('mark', 'a03_ball'),
        ('item',), ('events',),
        ('mark', 'a04_escape'),
        ('go', 12, 1, 'near'),
        ('mark', 'a05_12-1'),
        ('trig', 32, 124, A),
        ('wait', 120),
        ('mark', 'a06_map'),
    ]


def db5t():
    """The Airship's self destruct running out (from db5's mark `a04_escape`, work/db57/r5: the ball taken, s387's
    3-minute countdown running in 12-28): wait by 12-27's save point, walk out to the empty lift room 12-22 and let
    the countdown reach 0 there."""
    return [
        ('mark', 't00'),
        ('go', 12, 27, 'near'),
        ('waitcd', 2900),
        ('go', 12, 22, 'near'),
        ('mark', 't01_12-22'),
        ('waitcd', 0),
        ('mark', 't02_zero'),
        ('wait', 1500),
        ('mark', 't03_end'),
    ]


ROUTES = {'db5': db5, 'db5t': db5t}
