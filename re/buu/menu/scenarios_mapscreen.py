"""Map screen scenarios (menu/mapscreen.rs): SELECT in the field."""

SCENARIOS = {
    # Snakeway: the area page, R -> the world map, L back, R, B closes
    'map_snakeway': ('buu_snakeway_start.ss', None, ['-:2'], ['SELECT:1', '-:70', 'R:1', '-:50', 'L:1', '-:50', 'UP:20', 'LEFT:20', '-:10', 'B:1', '-:70']),
}
