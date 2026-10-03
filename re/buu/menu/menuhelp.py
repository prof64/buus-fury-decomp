"""Helpers for menu capture scenarios (RAM setup of the GameData block before the menu opens).

Scenario modules `scenarios_<area>.py` next to capture.py define `SCENARIOS = {name: (state, setup, pre, keys)}`:
state = save state in work/states, setup(e) = RAM pokes (or None), pre = key program before the recorded part,
keys = the recorded key program (frame 0 of the recording = its first frame; put the menu-opening key there)."""

GD = 0x03001068
REC = GD + 0x4F8
SUPPLIES = GD + 2
EQUIP_N = GD + 0xE4
EQUIP = GD + 0xE8
ZENIE = GD + 0x4F4


def rec(i):
    return REC + 0x68 * i


def setup_points(n, char=0):
    def f(e):
        e.w16(rec(char) + 0x2C, n)
    return f


def setup_party(chars):
    def f(e):
        for c in chars:
            e.w8(rec(c), e.r8(rec(c)) | 1)
    return f


def setup_supplies(counts):
    """{supply index: count}"""
    def f(e):
        for i, n in counts.items():
            e.w8(SUPPLIES + i, n)
    return f


def setup_equipment(items):
    """owned equipment list (item ids; | 0x10000000 marks an equipped entry)"""
    def f(e):
        e.w32(EQUIP_N, len(items))
        for k, it in enumerate(items):
            e.w32(EQUIP + 4 * k, it)
    return f


def setup_zenie(z):
    def f(e):
        e.w32(ZENIE, z)
    return f


def setup_flags(flags):
    def f(e):
        for fl in flags:
            a = GD + 0x55 + (fl >> 3)
            e.w8(a, e.r8(a) | (1 << (fl & 7)))
    return f


def chain(*fs):
    def f(e):
        for g in fs:
            if g:
                g(e)
    return f
