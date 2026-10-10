"""Trading Post scenarios (link cable; recorded by linkcap.py on two linked emulators, merged into
scenarios_link.LINK). Both sides start on the trade's picker (game 1 of run 2's save); A (the host) is recorded, B
plays the friend. "rich": a first trade where B gives A five things, then both come back into the Trading Post - A's
list then scrolls and has supplies with several.

The friend's side as A's game learned it goes into the rows (`link`, see `post`): the remake's harness hands it to
the page (wf_core::menu::link::harness) on the row where A's game acted on it."""
from scenarios_link import TO_TRADE, cursor_t

SCENARIOS = {}

PICKER = 0x0804A60C      # the Trading Post page (picker)
COUNT = 0x0804BC04       # "How many to trade?"
PROPOSE = 0x0804CAD4     # "Waiting for a friend..." after Propose Trade (the offers' exchange)
PROPOSAL = 0x0804B604    # Your Trade / Friend's Trade, ACCEPT / DECLINE
ANSWER = 0x0804C0F0      # "Waiting for a friend..." after ACCEPT / DECLINE (the answers' exchange)
COMPLETE = 0x08049B5C    # TRADE COMPLETE!
ANSWER_SESSION = 0x0804BC30  # the answer box's link session (+0x12C ours, +0x12D the friend's: 1 accept, 2 decline)

# B gives A five things (A on its first entry five times), both propose and accept; back on the Multiplayer page
# both choose Trading Post again, wait for the link, game 1, the picker
RICH = TO_TRADE + [('-', 'A', 7), ('-', '-', 30)] * 5 + [('R', 'R', 7), ('-', '-', 90), ('A', 'A', 7), ('-', '-', 400),
                                                         ('DOWN', 'DOWN', 7), ('-', '-', 30), ('A', 'A', 7), ('-', '-', 300),
                                                         ('A', 'A', 7), ('-', '-', 240)]


def prog(n, *events):
    """a key program of n frames: events (start frame, key, frames held)"""
    out, t = [], 0
    for s, k, d in sorted(events):
        assert s >= t, (s, t)
        if s > t:
            out.append(f'-:{s - t}')
        out.append(f'{k}:{d}')
        t = s + d
    if n > t:
        out.append(f'-:{n - t}')
    return out


def taps(t0, *seq, gap=14, hold=2):
    """presses from frame t0, one every `gap` frames ('-' skips a slot; 'K*n' holds n frames)"""
    ev = []
    for i, k in enumerate(seq):
        if k == '-':
            continue
        h = hold
        if '*' in k:
            k, h = k.split('*')
            h = int(h)
        ev.append((t0 + i * gap, k, h))
    return ev


def offer_at(e, page, off):
    n = min(e.r32(page + off), 8)
    return [[e.r32(page + off + 4 + 8 * k), e.r8(page + off + 8 + 8 * k), e.r8(page + off + 9 + 8 * k)] for k in range(n)]


def probe(e):
    """A's running mode (its vt) and what the trade's modes know of the friend: the friend's offer (picker +0xBCC,
    the proposal page's +0x114 is the picker) and the friend's answer (the answer box's link session object [0x03000FF0] +0x12D)"""
    m = e.r32(0x03000F90)
    vt = e.r32(m)
    o = {'vt': vt}
    if vt == PROPOSAL:
        o['fo'] = offer_at(e, e.r32(m + 0x114), 0xBCC)
    ses = e.r32(0x03000FF0)
    if ses and e.r32(ses) == ANSWER_SESSION:
        o['fa'] = e.r8(ses + 0x12D)
    return o


def post(frames, rows):
    """`link` on the row whose iteration ended a box because of the friend: the propose box -> the proposal page
    ({"offer": the friend's}), the answer box -> TRADE COMPLETE! / the picker ({"answer": 1 accept / 2 decline})"""
    fa = 0
    for k in range(1, len(frames)):
        a, b = frames[k - 1]['vt'], frames[k]['vt']
        fa = frames[k].get('fa') or fa
        if a == PROPOSE and b == PROPOSAL:
            rows[k]['link'] = {'offer': frames[k]['fo']}
        if a == ANSWER and b != ANSWER:
            rows[k]['link'] = {'answer': fa}
            fa = 0
    for r, f in zip(rows, frames):
        r['vt'] = f['vt']


def trade(keys, keys_b=None, flow=TO_TRADE):
    return dict(flow=flow, keys=keys, keys_b=keys_b or [], open={'kind': 'link_trade'}, probe0=cursor_t, probe=probe,
                post=post)


TRADE = {
    # the rich list: down to the end (it scrolls), back up; Curry Plate (2)'s count box (RIGHT to the limit, LEFT to 0,
    # A on 0, again and B), Zenie's count box held RIGHT (it speeds up) then LEFT, A
    'link_trade_scroll': trade(prog(900, *taps(20, *['DOWN'] * 8, *['UP'] * 8),
                                    *taps(260, 'DOWN', 'DOWN', 'DOWN', 'DOWN', 'A', 'RIGHT', 'RIGHT', 'LEFT', 'LEFT', 'LEFT',
                                          'A', '-', 'A', 'RIGHT', 'B', '-', 'DOWN', 'DOWN', 'DOWN', 'A', gap=16),
                                    (600, 'RIGHT', 120), (740, 'LEFT', 40), *taps(800, 'A', 'UP', 'UP'))),
    # picks onto "Your Trade:" (a count box for Curry Plate (2)), RIGHT over to it, its moves, A takes entries back
    # (a count box for the two), LEFT back, picks up to five (a sixth does not go)
    'link_trade_side': trade(prog(900, *taps(20, 'A', 'A', 'DOWN', 'A', 'RIGHT', 'A', 'A', 'A', 'RIGHT', 'DOWN', 'DOWN',
                                             'DOWN', 'UP', 'A', 'RIGHT', 'A', 'DOWN', 'A', 'LEFT', 'A', 'A', 'A', 'A', 'A',
                                             'A', 'DOWN', 'UP', gap=18),
                                  *taps(540, 'DOWN', 'DOWN', 'DOWN', 'A', 'A', 'UP', 'A', 'A', 'DOWN', 'A', gap=24))),
    # Propose Trade with one thing: the wait box, B cancels it; again, then the friend picks Zenie x3 and proposes:
    # the proposal page; RIGHT / LEFT between ACCEPT and DECLINE; B leaves it (the friend stays there)
    'link_trade_propose': trade(prog(560, *taps(20, 'A', '-', 'R', '-', '-', '-', 'B', '-', '-', 'R'),
                                     *taps(330, 'RIGHT', 'LEFT', 'LEFT', 'RIGHT', 'LEFT', '-', '-', 'B', 'DOWN', 'UP')),
                                prog(560, *taps(150, *['DOWN'] * 5, 'A', 'RIGHT', 'RIGHT', 'A', '-', 'R', hold=4))),
    # both propose (the friend first: A's wait box is short), A accepts: the wait box; the friend moves and accepts:
    # TRADE COMPLETE!, the save, the Multiplayer page
    'link_trade_accept': trade(prog(520, *taps(20, 'DOWN', 'A', '-', '-', '-', '-', 'R'), *taps(200, 'A')),
                               prog(520, *taps(20, 'A', '-', 'R', hold=4), *taps(260, 'RIGHT', 'LEFT', 'A', hold=4))),
    # A declines and waits for the friend's answer (accept): both back on the picker, the offers as they were
    'link_trade_decline': trade(prog(420, *taps(20, 'A', '-', 'R'), *taps(160, 'RIGHT', 'A'),
                                     *taps(280, 'DOWN', 'RIGHT', 'UP', 'LEFT', 'A')),
                                prog(420, *taps(20, 'A', '-', 'R', hold=4), *taps(220, 'A', hold=4))),
    # A accepts, the friend declines: back on the picker; then A leaves the picker (B) for Select Saved Game
    'link_trade_declined': trade(prog(480, *taps(20, 'A', '-', 'R'), *taps(160, 'A'), *taps(400, 'B')),
                                 prog(480, *taps(20, 'A', '-', 'R', hold=4), *taps(220, 'RIGHT', 'A', hold=4))),
    # Zenie's count box held RIGHT long: by 1, by 10 from 100, by 100 from 1000, up to the 1950 there are; held LEFT
    # down again, A; then the offer's Zenie back through the count box (B, then A with LEFT held to 0: nothing)
    'link_trade_count': trade(prog(1000, *taps(20, *['DOWN'] * 5, 'A'), (120, 'RIGHT', 520), (660, 'LEFT', 140),
                                   *taps(820, 'A', 'RIGHT', 'A', 'B', 'A', '-', '-', 'LEFT*30', '-', '-', 'A'))),
    # an empty proposal both ways (Propose Trade with nothing on the table), A leaves with B on the picker
    'link_trade_empty': trade(prog(360, *taps(25, 'R'), *taps(140, 'DOWN', 'RIGHT', 'A'), *taps(300, 'B')),
                              prog(360, *taps(30, 'R', hold=4), *taps(180, 'A', hold=4))),
}
for _k in ('link_trade_scroll', 'link_trade_side'):
    TRADE[_k]['flow'] = RICH
