"""Map screen scenarios (menu/mapscreen.rs): SELECT in the field."""

SCENARIOS = {
    # Snakeway: the area page, R -> the world map, L back, R, B closes
    'map_snakeway': ('buu_snakeway_start.ss', None, ['-:2'], ['SELECT:1', '-:70', 'R:1', '-:50', 'L:1', '-:50', 'UP:20', 'LEFT:20', '-:10', 'B:1', '-:70']),
}
SCENARIOS['map_left'] = ('buu_snakeway_start.ss', None, ['-:2'], ['SELECT:1', '-:60', 'L:1', '-:60', 'L:1', '-:20', 'R:1', '-:60', 'SELECT:1', '-:70'])


def _reveal(e):
    # game bit 0x40 (`set_game_bit_40`: every room of the record drawn, unvisited ones as rect1 / map1) and the world
    # page's unlock flags (#15: 13, 398, 400, 402 (= world 8, the current one: blinking), 403)
    e.w8(0x03001069, e.r8(0x03001069) | 0x40)
    for f in (13, 398, 400, 402, 403):
        a = 0x03001068 + 0x55 + (f >> 3)
        e.w8(a, e.r8(a) | (1 << (f & 7)))


SCENARIOS['map_forest'] = ('buu_swforest3.ss', _reveal, ['-:2'],
                           ['SELECT:1', '-:40', 'LEFT:60', 'UP:50', 'DOWN+RIGHT:90', 'RIGHT:40', '-:10', 'R:1', '-:60', 'A:1', '-:70'])
