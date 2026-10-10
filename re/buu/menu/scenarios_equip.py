"""Equip page scenarios (menu/equip.rs)."""
from menuhelp import EQUIP, EQUIP_N, rec, chain, setup_party


def owned(entries, wear=None):
    """entries: equipment ids; wear: {list index: char} marks (the char's slot is set too)"""
    wear = wear or {}

    def f(e):
        e.w32(EQUIP_N, len(entries))
        for k, it in enumerate(entries):
            v = it
            if k in wear:
                c = wear[k]
                v |= (c + 1) << 28
                slot = 0 if it < 44 else 1 if it < 77 else 2 if it < 112 else 3
                e.w32(rec(c) + 0x3C + 4 * slot, it)
            e.w32(EQUIP + 4 * k, v)
    return f


# body 1 Dirty Shirt, 2, 5 Cotton Gi; hands 45, 46; feet 78; accessory 113, 150 (Mercury's Cap: level 122)
LIST = [1, 45, 2, 78, 113, 5, 150, 46, 3, 4, 6, 7]

SCENARIOS = {
    # Status -> R R: the Equip page, slots UP / DOWN, the empty-list message, back
    'equip_page': ('buu_snakeway_start.ss', None, ['-:2'],
                   ['START:1', '-:40', 'R:2', '-:20', 'R:2', '-:24', 'DOWN:2', '-:14', 'DOWN:2', '-:14', 'DOWN:2', '-:14', 'DOWN:2', '-:14',
                    'UP:2', '-:14', 'A:2', '-:24', 'B:2', '-:24', 'L:2', '-:24', 'R:2', '-:24', 'R:2', '-:30', 'START:1', '-:40']),
    # owned equipment: browse a slot's list (preview stats, RqLvl), equip, swap, unequip, an item worn by Gohan, drop
    'equip_change': ('buu_snakeway_start.ss', chain(setup_party([1]), owned(LIST, {0: 0, 7: 1})), ['-:2'],
                     ['START:1', '-:40', 'R:2', '-:20', 'R:2', '-:24', 'A:2', '-:24', 'DOWN:2', '-:14', 'DOWN:2', '-:14', 'DOWN:2', '-:14',
                      'DOWN:2', '-:14', 'UP:2', '-:14', 'A:2', '-:24', 'DOWN:2', '-:14', 'UP:2', '-:14', 'A:2', '-:30',
                      # body list again: unequip the worn one
                      'A:2', '-:24', 'A:2', '-:24', 'DOWN:2', '-:14', 'A:2', '-:30', 'B:2', '-:24',
                      # hands: Gohan's gloves (equip takes them from him)
                      'DOWN:2', '-:14', 'A:2', '-:24', 'DOWN:2', '-:14', 'A:2', '-:24', 'A:2', '-:30',
                      # accessory: Mercury's Cap (level too low), cancel; drop the other one
                      'DOWN:2', '-:14', 'DOWN:2', '-:14', 'A:2', '-:24', 'DOWN:2', '-:14', 'A:2', '-:24', 'A:2', '-:20', 'B:2', '-:24',
                      'UP:2', '-:14', 'A:2', '-:24', 'DOWN:2', '-:14', 'DOWN:2', '-:14', 'A:2', '-:30', 'B:2', '-:24', 'B:2', '-:40']),
}
