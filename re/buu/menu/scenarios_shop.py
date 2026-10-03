"""Shop scenarios (menu/shop.rs): the screens are opened by injecting the script call (shop_add_item 0xCD ×n,
open_shop_buy 0xCE / open_shop_sell 0xCF) right before frame 0; the remake opens the same screen from meta "open"."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from menuhelp import setup_zenie, setup_supplies, setup_equipment, chain


def inject(items):
    def f(e):
        from script_nat_b_lib import asm, start_call, CODE_AT, HDR_AT, INTERP_AT
        code = asm(items)
        blob = b'\x01\x00\x00' + code
        blob += b'\x00' * (-len(blob) % 4)
        for i in range(0, len(blob), 4):
            e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
        e.w32(HDR_AT, CODE_AT)
        for i in range(0, 0x6c, 4):
            e.w32(INTERP_AT + i, 0)
        start_call(e, 0x08004098, (INTERP_AT, HDR_AT, 0, 0))
    return f


def buy(entries):
    """entries: (category, item, price, stock)"""
    items = []
    for c, i, p, s in entries:
        items += [c, i, p, s, ('n', 0xCD)]
    items += [('n', 0xCE)]
    return dict(open={'kind': 'shop_buy', 'items': [list(x) for x in entries]}, trigger=inject(items))


def sell(pct):
    return dict(open={'kind': 'shop_sell', 'percent': pct}, trigger=inject([pct, ('n', 0xCF)]))


S = 'buu_snakeway_start.ss'
LIST = [(0, 0, 50, 0), (0, 1, 120, 0), (1, 5, 300, 2), (0, 2, 75, 3), (1, 8, 900, 0)]

SCENARIOS = {
    # browse, count up / down, buy (money down, stock), not enough money, leave
    'shop_buy': (S, setup_zenie(1000), ['-:3'],
                 ['-:50', 'DOWN:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'LEFT:2', '-:12', 'A:2', '-:16', 'DOWN:2', '-:12',
                  'RIGHT:2', '-:12', 'A:2', '-:16', 'DOWN:2', '-:12', 'DOWN:2', '-:12', 'DOWN:2', '-:12', 'RIGHT:2', '-:12', 'A:2',
                  '-:16', 'UP:2', '-:12', 'UP:2', '-:12', 'B:2', '-:70'], buy(LIST)),
    # hold DOWN (auto-repeat) through a long list
    'shop_repeat': (S, setup_zenie(99999), ['-:3'],
                    ['-:50', 'DOWN:60', '-:16', 'UP:40', '-:16', 'B:2', '-:70'],
                    buy([(0, i, 10 * (i + 1), 0) for i in range(12)])),
    # sell supplies and equipment at 50 %
    'shop_sell': (S, chain(setup_zenie(100), setup_supplies({0: 3, 2: 1, 5: 12}), setup_equipment([5, 8, 5])), ['-:3'],
                  ['-:50', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'A:2', '-:16', 'DOWN:2', '-:12', 'A:2', '-:16', 'DOWN:2', '-:12',
                   'RIGHT:2', '-:12', 'A:2', '-:16', 'B:2', '-:70'], sell(50)),
    # sell equipment (scroll down past the supplies), the last copy leaves the list
    'shop_sell_eq': (S, chain(setup_zenie(100), setup_supplies({0: 1}), setup_equipment([5, 8, 5])), ['-:3'],
                     ['-:50', 'DOWN:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'A:2', '-:16', 'A:2', '-:16', 'DOWN:2', '-:12',
                      'A:2', '-:16', 'B:2', '-:70'], sell(25)),
}
