"""The full cutscene scenarios (recorded by `python scenarios.py full|<name>`): real event graphs from the game, started
naturally (new-game prologue, on-defeat callbacks) or injected through run_scene 0x08012588 (record_event).

Each entry: state, event (None = natural: the recording starts at the first scene step / run_scene), params, frames
(cap), keys(f) (indexed from the state for natural scenarios, from the scene start for injected ones), pre(e) (setup
after loading), tail (frames kept after the outer scene exits), how (meta text)."""
from capture import record_event, KEY, GD, WORK
from full import (record_steps_natural, record_injected, exit_stop, annotate, spawn_npc, lethal_hit, enemies,  # noqa
                  etype, keep_alive)


def mash_a(period=30, hold=4):
    return lambda f: KEY.A if f % period < hold else 0


def soft_reset(e):
    e.set_keys(KEY.A | KEY.B | KEY.START | KEY.SELECT)
    e.run(10)
    e.set_keys(0)
    e.run(5)


INTRO_START = 847   # frame (after soft_reset) at which New Game runs scene #4


def intro_keys(f):
    if f < INTRO_START - 20:
        # title -> New Game: alternate START / A every 40 frames
        if f % 40 < 4:
            return KEY.START if (f // 40) % 2 == 0 else KEY.A
        return 0
    if f < INTRO_START:
        return 0
    g = f - INTRO_START
    return KEY.A if g % 30 < 4 else 0


def at(seq):
    """keys from a {frame: key} table"""
    return lambda f: seq.get(f, 0)


def zenie(n):
    def pre(e):
        e.w32(GD + 0x4F4, n)
    return pre


def janemba_pre(e):
    """re/buu/script_trace_janemba2.py: the Vegeta NPC the warp skipped, through the game's own spawn path"""
    e.run(2)
    spawn_npc(e, 5, 170, 130)


def janemba_kill(e):
    boss = [q for q in enemies(e) if etype(e, q) == 0x33]
    lethal_hit(e, boss[0])


def first_enemy_kill(e):
    lethal_hit(e, enemies(e)[0])


FULL = {
    'intro': dict(
        state='buu_menus.ss', event=None, frames=7000, keys=intro_keys, pre=soft_reset, tail=30,
        how='buu_menus.ss, soft reset (A+B+START+SELECT), title -> New Game by alternating START/A every 40 frames; '
            'the prologue scene #4 starts at state 0 (run_scene from the new-game hook); from then A is pressed '
            '4 of every 30 frames (text boxes; the difficulty menu #516 inside #38 gets the default option); '
            '#4 -> goto #38 -> run_event #516 -> warp 0x77 -> batch walks -> exit'),
    'npc_choice': dict(
        state='buu_snakeway_start.ss', event=74, frames=600,
        keys=at({40: KEY.A, 80: KEY.A, 120: KEY.DOWN, 150: KEY.A, 200: KEY.A, 250: KEY.A, 300: KEY.A}),
        how='NPC talk #74 ("Want some fighting tips?", a kind-5 text box with a page break whose last page is a '
            '- Yes / - No choice) injected on Snake Way; A at 40 / 80 (pages), DOWN at 120, A at 150 -> result 1 '
            '("No") -> node 5 text -> A -> end'),
    'shop': dict(
        state='buu_snakeway_start.ss', event=34, frames=900, pre=zenie(20000),
        keys=at({40: KEY.A, 90: KEY.A, 160: KEY.A, 220: KEY.B, 360: KEY.B, 420: KEY.DOWN, 440: KEY.DOWN,
                 470: KEY.A, 540: KEY.A, 600: KEY.A}),
        how='shop #34 injected on Snake Way, zenie poked to 20000 (GameData+0x4F4): choice Buy (A) -> choice Items '
            '(A) -> 8 x shop_add_item + open_shop_buy -> A buys the first item (3000 Z, SFX 41) -> B leaves the '
            'shop -> B cancels the Items/Equipment choice (result -1 -> back to node 0) -> DOWN DOWN A = End -> '
            '"Thank you" text -> A -> end'),
    'janemba_handover': dict(
        state=str(WORK / 'hfil_janemba_alive.ss'), event=None, inject=janemba_kill, pre=janemba_pre, frames=3000,
        keys=mash_a(), tail=60,
        how='work/buu/hfil_janemba_alive.ss (HFIL 18-14, natural Janemba); Vegeta NPC (script actor 5) spawned through '
            'the game spawn path (0x08000158/0x0801111C/0x0801191A) at (170, 130); frame 0 = the injected lethal hit '
            '0x0800FA2A on Janemba, whose on-defeat callback runs scene #450 in that frame; A 4 of every 30 frames'),
    'buu_form': dict(
        state='buu_snakeway_start.ss', event=471, frames=8000, keys=mash_a(),
        how='cutscene #471 injected on Snake Way (it loads its own scene: load_scene(20, 5, 1, 104, 83) + '
            'spawn_actor): Gohan vs Super Buu, Buu absorbs Gohan = change_sprite(51, 52) (Super Buu -> Super Buu '
            'with Gohan), then goto #481 (another scene load); A 4 of every 30 frames'),
    'boss_defeat': dict(
        state=str(WORK / 'yakon_map40.ss'), event=None, inject=first_enemy_kill, frames=4000, keys=mash_a(),
        tail=60,
        how='work/buu/yakon_map40.ss (Yakon fight, map 40); frame 0 = injected lethal hit 0x0800FA2A on Yakon; the '
            'kill XP runs the level-up hook #3 first (kind-6 text with printf args), then Yakon\'s on-defeat '
            'scene #329 (camera_center_on, shakes, fades, remove_actor, spawn/jump/walk, warp); A 4 of every 30'),
}


def record_full(s):
    if s.get('inject'):
        return record_injected(s['state'], s['frames'], s.get('keys'), s.get('pre'), s['inject'],
                               stop=exit_stop(s.get('tail', 30)))
    if s.get('event') is None:
        e, doc = record_steps_natural(s['state'], s['frames'], s.get('keys'), s.get('pre'),
                                      stop=exit_stop(s.get('tail', 30)))
        return e, doc
    e, doc = record_event(s['state'], s['event'], s.get('params', (0, 0, 0, 0)), frames=s.get('frames', 3000),
                          keys=s.get('keys'), pre=s.get('pre'), tail=s.get('tail', 8))
    annotate(doc)
    return e, doc
