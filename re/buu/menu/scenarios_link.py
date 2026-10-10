"""Link-cable screen scenarios (recorded by linkcap.py on two linked emulators; not capture.py's - SCENARIOS is
empty here). Both sides boot with run 2's save: game 1 (Goku / Vegeta in the party), game 2 empty. `flow`: (A keys,
B keys, frames) up to the recording; `keys` / `keys_b`: the recorded key programs; `open`: the page the remake
opens at frame 0 (wf_core::menu::link::open_json)."""
SCENARIOS = {}

# title -> Multiplayer -> the link box until the cable is up -> Dojo (A on the first item) -> Select Saved Game
TO_SAVES_DOJO = [('-', '-', 1100), ('A', 'A', 7), ('-', '-', 40), ('DOWN', 'DOWN', 7), ('-', '-', 30), ('A', 'A', 7),
                 ('-', '-', 300), ('A', 'A', 7), ('-', '-', 120)]
TO_SAVES_TRADE = TO_SAVES_DOJO[:-2] + [('DOWN', 'DOWN', 7), ('-', '-', 30), ('A', 'A', 7), ('-', '-', 120)]
# game 1 on both -> the Dojo's setup page (the host's) / the trade's picker
TO_SETUP = TO_SAVES_DOJO + [('A', 'A', 7), ('-', '-', 120)]
# the setup's START FIGHT (both have picked their saves): the character pick on both sides
TO_CHARS = TO_SETUP + [('A', '-', 7), ('-', '-', 150)]
TO_TRADE = TO_SAVES_TRADE + [('A', 'A', 7), ('-', '-', 240)]


def cursor_t(e, o, ahead=1):
    """the running mode's cursor child (vt 0x08049898): its blink counter, as the title scenarios take it (`ahead`:
    one step ahead in the next iteration for a mode's own cursor - the title page's under the link flow is read as
    is: menu_parity link_connect)"""
    m = e.r32(0x03000F90)
    for j in range(min(e.r32(m + 4), 64)):
        c = e.r32(m + 8 + 4 * j)
        if e.r32(c) == 0x08049898:
            o['cursor_t'] = e.r32(c + 8) + ahead


def figs(e, o):
    """the list's party figures (character objects, vt 0x0804AC20, in the object list 0x03001C4C): their animation
    ticks (+0x12) in list order - the remake starts its figures there"""
    n = min(e.r32(0x03001C4C + 4), 64)
    obs = [e.r32(0x03001C4C + 8 + 4 * k) for k in range(n)]
    o['fig_t'] = [e.r16(ob + 0x12) for ob in obs if e.r32(ob) == 0x0804AC20]


def list_sel(e, o):
    """the game list mode's selected row (+0x10C, as the title's) and the cursor's blink"""
    o['sel'] = e.r32(e.r32(0x03000F90) + 0x10C)
    cursor_t(e, o)
    figs(e, o)


def setup_probe(e, o):
    """the Dojo setup page (vt 0x0804A3BC): the selected control (+0x10C), the cursor's blink, the boxes' colours
    (+0x114.. -> box +0x38) and the link config (`[0x0872C34C + 4]`: byte 0 = Balanced, +4 the rounds code)"""
    m = e.r32(0x03000F90)
    if e.r32(m) == 0x0804B550:
        # (the wait box is up: its parent, the page under it, is the previous mode)
        o['wait'] = True
    o['sel'] = e.r32(m + 0x10C)
    cursor_t(e, o)
    cfg = e.r32(0x0872C34C + 4)
    o['balanced'] = e.r8(cfg) != 0
    o['rounds'] = e.r32(cfg + 4)


def chars_probe(e, o):
    """the character pick (vt 0x0804CAA8): the selection (+0x11C), the mode (config byte 0) and the five figures
    (+0x12C..: character objects - walk (anim pointer = the sprite's anim 1), cel +0x11, tick +0x12)"""
    m = e.r32(0x03000F90)
    o['sel'] = e.r32(m + 0x11C)
    o['balanced'] = e.r8(0x03002F58) != 0
    figs = []
    for k in range(5):
        ob = e.r32(m + 0x12C + 4 * k)
        if not ob:
            figs.append(None)
            continue
        d = e.r32(ob + 0x58)
        figs.append([int(e.r32(ob + 0x50) == e.r32(d + 0x58)), e.r8(ob + 0x11), e.r16(ob + 0x12)])
    o['figs'] = figs


def keys(*seq, gap=12):
    out = []
    for k in seq:
        out += [f'{k}:2', f'-:{gap}']
    return out


LINK = {
    # Multiplayer -> Dojo on both: the title's link box, the cable up, the box's count, the Select Saved Game list in
    'link_connect': dict(flow=TO_SAVES_DOJO[:-2], keys=['-:10', 'A:7', '-:200'], keys_b=['-:10', 'A:7'], up=True,
                         open={'kind': 'title', 'page': 'multi'}, probe0=lambda e, o: cursor_t(e, o, 0)),
    # Select Saved Game: the list (game 1's party figures animate), the cursor down to the empty game and back
    'link_saves': dict(flow=TO_SAVES_DOJO, keys=['-:20'] + keys('DOWN', 'UP', 'DOWN', 'UP') + ['-:30'],
                       open={'kind': 'link_saves', 'trade': False}, probe0=list_sel),
    # the Dojo's setup page, host: Character Mode (Balanced / In-Game), Rounds (1 / 3 / 5), Start Fight
    'link_setup': dict(flow=TO_SETUP, keys=['-:20'] + keys('UP', 'UP', 'RIGHT', 'LEFT', 'A', 'DOWN', 'RIGHT', 'RIGHT',
                                                           'LEFT', 'A', 'DOWN', 'UP') + ['-:30'],
                       open={'kind': 'link_setup', 'host': True}, probe0=setup_probe),
    # the host's setup before the guest has picked its save: START FIGHT -> "Waiting for a friend..." over the page,
    # B cancels it, START FIGHT again; the guest picks its save: the box ends, the character pick
    'link_setup_wait': dict(flow=TO_SAVES_DOJO + [('A', '-', 7), ('-', '-', 120)],
                            keys=['-:10', 'A:2', '-:40', 'B:2', '-:30', 'A:2', '-:100'],
                            keys_b=['-:120', 'A:2'], open={'kind': 'link_setup', 'host': True},
                            probe0=setup_probe),
    # B on the setup page: back to Select Saved Game, A: the setup again
    'link_setup_back': dict(flow=TO_SETUP, keys=['-:10', 'B:2', '-:60', 'A:2', '-:60'],
                            open={'kind': 'link_setup', 'host': True}, probe0=setup_probe),
    # the character pick, host, Balanced: RIGHT / LEFT through the roster (the figures scroll), A -> the wait box; the
    # guest picks: the fight starts (the fade out). (The moves come while the walker shows cel 0 or 1: a move at its
    # cel 2 / 3 draws the idled figure with a cel past the idle animation's - a read past its cel table, not
    # reproduced; engine/docs/accuracy-log.md)
    'link_chars': dict(flow=TO_CHARS, keys=['-:14'] + keys('RIGHT', 'RIGHT', 'RIGHT', 'LEFT', gap=40) + ['-:20', 'A:2', '-:120'],
                       keys_b=['-:280', 'A:2'], open={'kind': 'link_chars', 'host': True}, probe0=chars_probe),
    # the guest's Select Saved Game while the host sets up: A -> the wait box over the list, B cancels it, A again;
    # the host's START FIGHT ends it (the character pick)
    'link_guest_wait': dict(flow=TO_SAVES_DOJO + [('A', '-', 7), ('-', '-', 60)], subject='b',
                            keys=['-:10', 'A:2', '-:40', 'B:2', '-:30', 'A:2', '-:100'], keys_b=['-:110', 'A:2'],
                            open={'kind': 'link_saves', 'trade': False}, probe0=list_sel),
    # the trade's picker: down the list, an item onto "Your Trade:", zenie's count box (RIGHT up, LEFT down), cancel
    'link_trade_list': dict(flow=TO_TRADE, keys=['-:20'] + keys('DOWN', 'DOWN', 'UP', 'A', 'DOWN', 'DOWN', 'DOWN', 'A',
                                                                 'RIGHT', 'RIGHT', 'LEFT', 'A') + ['-:30'],
                            open={'kind': 'link_trade'}),
}

# the Trading Post's pages (scenarios_trade.py)
from scenarios_trade import TRADE, probe as _trade_probe, post as _trade_post  # noqa: E402
LINK['link_trade_list'].update(probe0=cursor_t, probe=_trade_probe, post=_trade_post)
LINK.update(TRADE)
