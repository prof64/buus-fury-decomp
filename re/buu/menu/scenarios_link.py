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
                       open={'kind': 'link_setup', 'host': True}),
    # the trade's picker: down the list, an item onto "Your Trade:", zenie's count box (RIGHT up, LEFT down), cancel
    'link_trade_list': dict(flow=TO_TRADE, keys=['-:20'] + keys('DOWN', 'DOWN', 'UP', 'A', 'DOWN', 'DOWN', 'DOWN', 'A',
                                                                 'RIGHT', 'RIGHT', 'LEFT', 'A') + ['-:30'],
                            open={'kind': 'link_trade'}),
}

# the Trading Post's pages (scenarios_trade.py)
from scenarios_trade import TRADE, probe as _trade_probe, post as _trade_post  # noqa: E402
LINK['link_trade_list'].update(probe0=cursor_t, probe=_trade_probe, post=_trade_post)
LINK.update(TRADE)
