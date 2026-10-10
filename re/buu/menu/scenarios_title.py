"""Title scenarios (menu/title.rs): from save states at the idle title (buu_title.ss: no saves; buu_title_saved.ss:
Goku + Gohan + Goten saved in slot 1 on Snakeway). The remake opens the title at frame 0 (meta "open"), the title
cursor's blink phase taken from the emulator (`cursor_t`), and gets the save slots (meta "saves", decoded from the
game's EEPROM cache `[0x03004A64] + 0x10 + 0x8B0 k`: the 0x8A8-byte GameData with the location at +0x8A0)."""


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def _u(b, o, n):
    return int.from_bytes(b[o:o + n], 'little')


def rec_b(b, i):
    a = 0x4F8 + 0x68 * i
    n = b[a + 0xC]
    return dict(
        flags=b[a], fusion=b[a + 2], ss_cooldown=_u(b, a + 4, 2), form=b[a + 0x14], sel=b[a + 0x15],
        hp=s16(_u(b, a + 0x16, 2)), max_hp=s16(_u(b, a + 0x18, 2)), ep=s16(_u(b, a + 0x1A, 2)), max_ep=s16(_u(b, a + 0x1C, 2)),
        xp=_u(b, a + 0x24, 4), str=b[a + 0x28], pow=b[a + 0x29], end=b[a + 0x2A], spd=b[a + 0x2B], points=_u(b, a + 0x2C, 2),
        skills=[b[a + 0xD + k] for k in range(n)], skill_levels=[b[a + 0x2E + k] for k in range(n)],
        skill_cur=[b[a + 0x2E + 2 * k] for k in range(n)], skill_max=[b[a + 0x2F + 2 * k] for k in range(n)],
        equip=[_u(b, a + 0x3C + 4 * k, 4) for k in range(4)])


def slot_b(b):
    # `0x080048A0`: a slot whose Adler-32 (`0x0803FC4E`, at +0x8A8) is wrong reads as a fresh game (byte 1 = 0)
    import zlib
    if zlib.adler32(bytes(b[:0x8A8])) != _u(b, 0x8A8, 4) or not b[1] & 1:
        return None
    n = _u(b, 0xE4, 4)
    return dict(
        current=b[0], game_bits=b[1], supplies=list(b[2:2 + 83]), flags=b[0x55:0x55 + 0x8F].hex(),
        equipment=[_u(b, 0xE8 + 4 * i, 4) for i in range(min(n, 250))], zenie=_u(b, 0x4F4, 4),
        play_base=_u(b, 0x4D0, 4), counter=0, chars=[rec_b(b, i) for i in range(9)],
        world=b[0x8A0], area=b[0x8A1], variant=b[0x8A2], x=_u(b, 0x8A4, 2), y=_u(b, 0x8A6, 2), flag=bool(b[1] & 8))


def title_open(page='title'):
    meta = {'open': {'kind': 'title', 'page': page}}

    def trig(e):
        # the running mode's cursor child (vt 0x08049898): its blink counter
        m = e.r32(0x03000F90)
        n = e.r32(m + 4)
        for j in range(min(n, 64)):
            o = e.r32(m + 8 + 4 * j)
            if e.r32(o) == 0x08049898:
                # the counter is one step ahead in the mode's next iteration (measured); at a multiple of 8 the
                # redraw it asked for is in progress: the previous picture (8 steps earlier) is on screen
                t = e.r32(o + 8) + 1
                meta['open']['cursor_t'] = t
                if t % 8 == 0:
                    meta['open']['prev'] = {'kind': 'title', 'page': page, 'cursor_t': t - 8, 'sel': e.r32(m + 0x10C)}
        meta['open']['sel'] = e.r32(m + 0x10C)
        base = e.r32(0x03004A64)
        meta['saves'] = [slot_b(e.read(base + 0x10 + 0x8B0 * k, 0x8B0)) for k in range(3)]
    meta['trigger'] = trig
    return meta


T = 'buu_title.ss'
TS = 'buu_title_saved.ss'

def _vt(e):
    return dict(vt=e.r32(e.r32(0x03000F90)))


def _rtt(frames, extra):
    """the row whose iteration ran `return_to_title` (the title's mode, vt 0x0804A404, made): `rtt_row`"""
    extra['rtt_row'] = next(i for i, f in enumerate(frames) if f.get('vt') == 0x0804A404)


SCENARIOS = {
    # the game over (work/states/buu_died.ss = run 2's death, on the death scene's "You have died!"): A, the script's
    # fade-out, `return_to_title` - the title's first picture and its cursor's blink (menu_parity gives the remake the
    # return at `rtt_row`; the scene's frames before it are not compared)
    # the soft reset (A + B + SELECT + START) on the title: the boot screens (legal, logos), then the title again - its
    # first picture and its cursor's blink from the boot
    'title_boot': (T, None, ['-:7'], ['-:10', 'A+B+SELECT+START:2', '-:798'], title_open()),
    'title_gameover': ('buu_died.ss', None, [], ['-:31', 'A:3', '-:142'], {'rtt': 'game_over', 'probe': _vt, 'finish': _rtt}),
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
    # OPTIONS (vt 0x0804A430): each slider down and up (Sound FX, Music, Text Speed), the cursor round the rows and onto
    # Credits, A there (the credits as a nested mode; START does not skip them, A does), B back to the title, then START / OPTIONS
    # moves on the title again
    'title_options': (T, None, ['-:7'],
                      ['-:20', 'DOWN:7', '-:20', 'A:7', '-:60',
                       'LEFT:7', '-:20', 'RIGHT:7', '-:20', 'RIGHT:7', '-:20', 'DOWN:7', '-:20',
                       'LEFT:7', '-:20', 'RIGHT:7', '-:20', 'DOWN:7', '-:20',
                       'RIGHT:7', '-:20', 'LEFT:7', '-:20', 'DOWN:7', '-:20', 'DOWN:7', '-:20', 'UP:7',
                       '-:20', 'A:7', '-:120', 'START:7', '-:120', 'UP:7', '-:20', 'A:7', '-:20', 'B:7', '-:60',
                       'DOWN:7', '-:20', 'UP:7', '-:40'], title_open()),
    # Delete Game with only empty slots: A on each (nothing to delete), back to Select Game, B to the title
    'title_delete_empty': (T, None, ['-:7'], ['-:20', 'A:7', '-:40', 'A:7', '-:60', 'R:7', '-:30', 'A:7', '-:40', 'DOWN:7',
                                             '-:30', 'A:7', '-:40', 'L:7', '-:30', 'B:7', '-:40', 'B:7', '-:66'], title_open()),
    # New Game into the empty slot 2 beside the saved slot 1
    'title_newgame2': (TS, None, ['-:7'], ['-:20', 'A:7', '-:40', 'A:7', '-:60', 'DOWN:7', '-:30', 'A:7', '-:200'], title_open()),
    # Multiplayer with no link cable: its page (Dojo / Trading Post), the cursor, B back, B to the title
    'title_multi': (T, None, ['-:7'], ['-:20', 'A:7', '-:40', 'DOWN:7', '-:30', 'A:7', '-:60', 'DOWN:7', '-:30', 'UP:7',
                                      '-:30', 'B:7', '-:60', 'B:7', '-:66'], title_open()),
    # Multiplayer -> Dojo with no link cable
    'title_dojo': (T, None, ['-:7'], ['-:20', 'A:7', '-:40', 'DOWN:7', '-:30', 'A:7', '-:60', 'A:7', '-:240', 'B:7', '-:120',
                                     'B:7', '-:126'], title_open()),
    # Multiplayer -> Trading Post with no link cable
    'title_trade': (T, None, ['-:7'], ['-:20', 'A:7', '-:40', 'DOWN:7', '-:30', 'A:7', '-:60', 'DOWN:7', '-:30', 'A:7', '-:240',
                                      'B:7', '-:120', 'B:7', '-:120'], title_open()),
    # Delete Game on the saved slot: Are you sure? -> Cancel, then Delete
    'title_delete': (TS, None, ['-:7'], ['-:20', 'A:7', '-:40', 'A:7', '-:60', 'R:7', '-:30', 'A:7', '-:40', 'B:7', '-:40',
                                       'A:7', '-:40', 'A:7', '-:60', 'L:7', '-:30', 'B:7', '-:60'], title_open()),
}
