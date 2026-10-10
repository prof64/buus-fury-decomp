"""The Computer Room's computer (4-22, variant 1; a trigger of ctor 0x08007543 / vt 0x0804C11C, activated by A facing
up: `0x080074EC` fades out (0x080496CC, 16), saves the world, runs the map page vt 0x0804A950 (ctor 0x08024F3C),
restores the world and fades in (0x080495B4)). State buu_computer.ss: run 2 row 74,990 (tools/run2/state_at.py),
then walked Lobby -> Dr. Brief's Lab -> Computer Room (tools/story_playthrough/drive.py), facing the computer."""

S = 'buu_computer.ss'
# the remake builds the field at the capture's place; the player's facing (up) matters for the A activation
LIVE = {'live_field': True}


def _scouted(n):
    # the scouted bits (GD +0xC5 + db / 8: db 0 .. 0xE7; ctor 0x08024F3C lists the set ones)
    def f(e):
        for db in range(n):
            a = 0x03001068 + 0xC5 + (db >> 3)
            e.w8(a, e.r8(a) | (1 << (db & 7)))
    return f


SCENARIOS = {
    # A at the computer, the D-pad, L / R, then B
    'computer_map': (S, None, ['-:2'], ['A:1', '-:90', 'LEFT:30', '-:10', 'UP:30', '-:10', 'RIGHT:30', '-:10', 'DOWN:30', '-:10',
                                        'R:1', '-:40', 'L:1', '-:40', 'B:1', '-:90'], LIVE),
    # every entry scouted: RIGHT / LEFT through the entries (taps and a hold), the description scrolled, B
    'computer_db': (S, _scouted(0xE8), ['-:2'], ['A:1', '-:90', 'RIGHT:1', '-:40', 'RIGHT:1', '-:40', 'RIGHT:1', '-:40',
                                                  'LEFT:1', '-:40', 'DOWN:40', '-:20', 'UP:20', '-:20', 'RIGHT:60', '-:40',
                                                  'LEFT:1', '-:40', 'A:1', '-:60', 'B:1', '-:90'], LIVE),
    # every entry scouted: down the list to Babidi (3 variants: RIGHT round and LEFT back), on to Gohan (9 variants:
    # characters - the party's stats, portraits), the description scrolled (DOWN / UP held), B to the list, B closes
    'computer_variants': (S, _scouted(0xE8), ['-:2'], ['A:1', '-:80'] + ['DOWN:1', '-:7'] * 8 + ['-:20']
                          + ['RIGHT:1', '-:20'] * 4 + ['LEFT:1', '-:20'] * 2 + ['DOWN:1', '-:5'] * 30 + ['-:20']
                          + ['RIGHT:1', '-:14'] * 10 + ['A:1', '-:40', 'DOWN:60', '-:10', 'UP:30', '-:10', 'B:1', '-:40',
                                                         'B:1', '-:90'], LIVE),
    # a few entries: wrap at the ends
    'computer_few': (S, _scouted(3), ['-:2'], ['A:1', '-:90', 'LEFT:1', '-:40', 'RIGHT:1', '-:40', 'RIGHT:1', '-:40',
                                                'RIGHT:1', '-:40', 'B:1', '-:90'], LIVE),
}
