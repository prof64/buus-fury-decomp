"""Inventory page scenarios (menu/inventory.rs)."""
from menuhelp import setup_supplies, setup_equipment, chain, rec


def hp(n, ep=None):
    def f(e):
        e.w16(rec(0) + 0x16, n)
        if ep is not None:
            e.w16(rec(0) + 0x1A, ep)
    return f


ITEMS = {0: 1, 1: 3, 2: 5, 3: 1, 4: 2, 10: 1, 17: 12, 21: 2, 24: 1, 26: 1, 40: 1, 33: 1}
G = ['-:12']

SCENARIOS = {
    # Status -> R: the inventory with nothing in it, the story items tab, back
    'inventory_empty': ('buu_snakeway_start.ss', None, ['-:2'],
                        ['START:1', '-:40', 'R:2', '-:20', 'RIGHT:2', '-:12', 'LEFT:2', '-:12', 'R:2', '-:20', 'L:2', '-:20', 'B:2', '-:40']),
    # a long supply list: scrolling down / up (repeating buttons held), the story items tab
    'inventory_scroll': ('buu_snakeway_start.ss', chain(setup_supplies(ITEMS), hp(70, 40)), ['-:2'],
                         ['START:1', '-:40', 'R:2', '-:20'] + ['DOWN:2', '-:10'] * 7 + ['DOWN:40', '-:12', 'UP:3', '-:10', 'UP:30', '-:12',
                          'RIGHT:2', '-:12', 'DOWN:2', '-:12', 'LEFT:2', '-:12', 'L:2', '-:20', 'R:2', '-:20', 'START:1', '-:40']),
    # item use: Rice Ball (+40 HP), the dialog's UP / DOWN, cancel, use a Milk (EP), use the last Cookie (list shrinks)
    'inventory_use': ('buu_snakeway_start.ss', chain(setup_supplies(ITEMS), hp(60, 30)), ['-:2'],
                      ['START:1', '-:40', 'R:2', '-:20', 'DOWN:2', '-:12', 'A:2', '-:20', 'DOWN:2', '-:12', 'UP:2', '-:12', 'A:2', '-:30',
                       'A:2', '-:20', 'B:2', '-:30', 'DOWN:2', '-:10', 'DOWN:2', '-:10', 'DOWN:2', '-:10', 'DOWN:2', '-:10', 'A:2', '-:20', 'A:2', '-:30']
                      + ['DOWN:2', '-:10'] * 3 + ['A:2', '-:20', 'A:2', '-:30', 'RIGHT:2', '-:12', 'A:2', '-:20', 'A:2', '-:20', 'B:2', '-:20', 'B:2', '-:40']),
    # Senzu Bean (HP and EP to full), Holy Water (30 s of +25 STR / POW / END / SPD), an Onigiri at full HP, then L to
    # the Status page (the boosted stats), START out
    'inventory_use_senzu': ('buu_snakeway_start.ss', chain(setup_supplies({4: 1, 20: 1, 21: 2}), hp(60, 30)), ['-:2'],
                            ['START:1', '-:40', 'R:2', '-:20', 'DOWN:2', '-:12', 'DOWN:2', '-:12', 'A:2', '-:20', 'A:2', '-:30',
                             'UP:2', '-:12', 'A:2', '-:20', 'A:2', '-:30', 'UP:2', '-:12', 'A:2', '-:20', 'A:2', '-:30',
                             'L:2', '-:40', 'START:1', '-:40']),
    # Milk with the Primordial Twisty-Straw (equipment 118) owned: the game's bug, HP = EP + the bonus
    'inventory_use_straw': ('buu_snakeway_start.ss', chain(setup_supplies({10: 2}), setup_equipment([118]), hp(60, 30)), ['-:2'],
                            ['START:1', '-:40', 'R:2', '-:20', 'A:2', '-:20', 'A:2', '-:30', 'A:2', '-:20', 'A:2', '-:30',
                             'L:2', '-:40', 'START:1', '-:40']),
    # a Rice Ball with the Majestic Chopsticks (119) worn: +25 %
    'inventory_use_chop': ('buu_snakeway_start.ss', chain(setup_supplies({1: 2}), setup_equipment([0x10000000 | 119]),
                                                          lambda e: e.w32(rec(0) + 0x3C + 12, 119), hp(60, 30)), ['-:2'],
                           ['START:1', '-:40', 'R:2', '-:20', 'A:2', '-:20', 'A:2', '-:30', 'L:2', '-:40', 'START:1', '-:40']),
    # Milk with the Twisty-Straw worn (slot 4): the script's equipped branch, HP = EP + 25 (the game's own slip)
    'inventory_use_straw_worn': ('buu_snakeway_start.ss', chain(setup_supplies({10: 2}), setup_equipment([0x10000000 | 118]),
                                                                lambda e: e.w32(rec(0) + 0x3C + 12, 118), hp(60, 30)), ['-:2'],
                                 ['START:1', '-:40', 'R:2', '-:20', 'A:2', '-:20', 'A:2', '-:30', 'L:2', '-:40', 'START:1', '-:40']),
}
