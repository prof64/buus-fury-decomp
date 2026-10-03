"""Title scenarios (menu/title.rs): from save states at the idle title (buu_title.ss: no saves; buu_title_saved.ss:
Goku + Gohan + Goten saved in slot 1 on Snakeway). The remake opens the title at frame 0 (meta "open"), the title
cursor's blink phase taken from the emulator (`cursor_t`)."""


def title_open(page='title'):
    meta = {'open': {'kind': 'title', 'page': page}}

    def trig(e):
        # the running mode's cursor child (vt 0x08049898): its blink counter
        m = e.r32(0x03000F90)
        n = e.r32(m + 4)
        for j in range(min(n, 64)):
            o = e.r32(m + 8 + 4 * j)
            if e.r32(o) == 0x08049898:
                meta['open']['cursor_t'] = e.r32(o + 8)
        meta['open']['sel'] = e.r32(m + 0x10C)
    meta['trigger'] = trig
    return meta


T = 'buu_title.ss'
TS = 'buu_title_saved.ss'

SCENARIOS = {
    # move between START / OPTIONS, START -> Single / Multiplayer -> back (B) -> START -> Single -> Select Game
    # (two empty slots), move, toggle Delete Game (L / R) and back, B -> back
    'title_menus': (T, None, ['-:7'],
                    ['-:20', 'DOWN:7', '-:20', 'UP:7', '-:20', 'A:7', '-:40', 'DOWN:7', '-:20', 'UP:7', '-:20', 'B:7', '-:40',
                     'START:7', '-:40', 'A:7', '-:60', 'DOWN:7', '-:24', 'DOWN:7', '-:24', 'UP:7', '-:24', 'R:7', '-:24',
                     'DOWN:7', '-:24', 'L:7', '-:24', 'B:7', '-:60'], title_open()),
    # New Game into slot 1
    'title_newgame': (T, None, ['-:7'], ['-:20', 'A:7', '-:40', 'A:7', '-:60', 'A:7', '-:200'], title_open()),
    # a saved slot: its summary, LEFT / RIGHT through the party, Continue
    'title_continue': (TS, None, ['-:7'], ['-:20', 'A:7', '-:40', 'A:7', '-:60', 'RIGHT:7', '-:24', 'RIGHT:7', '-:24', 'LEFT:7',
                                         '-:24', 'A:7', '-:200'], title_open()),
    # Delete Game on the saved slot: Are you sure? -> Cancel, then Delete
    'title_delete': (TS, None, ['-:7'], ['-:20', 'A:7', '-:40', 'A:7', '-:60', 'R:7', '-:30', 'A:7', '-:40', 'B:7', '-:40',
                                       'A:7', '-:40', 'A:7', '-:60', 'L:7', '-:30', 'B:7', '-:60'], title_open()),
}
