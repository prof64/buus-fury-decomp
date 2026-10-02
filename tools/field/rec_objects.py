"""Field objects ground truth: breakables + loot, map item pickups, touch / A-button triggers, save points.

    .venv/bin/python tools/field/rec_objects.py [NAME_GLOB ...]      -> data/field/objects_<name>.json

Headless, muted emulator (tools/emu). Each scenario: a base save state, a setup (warp, position, facing), then
frame 0 = the first recorded frame; per frame the input is applied, one video frame runs, and the state after it
is recorded. Rules decoded from the game: re/buu/notes.md "Field: triggers, breakables, pickups, game state".

Trace format (all positions in px unless noted):
  name, source_state, notes, area [world, area, variant]
  init: rng [s0, s1], player {x, y (24.8), facing (0 down 1 up 2 left 3 right)}, flags_hex (GameData+0x55, 143
        bytes), zenie, supplies {id: count}, equipment [entries], objs (frame-0 object list, see objs), triggers
  input: [[keys, frames], ...] (GBA key mask: A 1, B 2, RIGHT 16, LEFT 32, UP 64, DOWN 128)
  frames: [{f, keys, logic (game logic ran: frame counter 0x03000F94 advanced), rng, p [x, y, facing, state],
            area, objs, zenie, supplies_changed {id: count}, equipment_n, triggers (only when the list changed)}]
     objs: [kind, vtable, ...] per object in world-list order (update order is last to first):
        breakable [.., x0, y0, x1, y1, hp, shake_left, hit_dir]
        coin      [.., x (24.8), y (24.8), z (24.8, drawn y = y - z>>8), moving_frames (+0x2C), age (+0x24),
                   x0, y0, x1, y1 (pickup box)]
        drop_item [.., item (>=0 supply, <0 -equipment), x (24.8), y (24.8), z (22.10), vz, landed, x0, y0, x1, y1]
        map_item  [.., supply def index, x0, y0, x1, y1 (solid / pickup box)]
        player/enemy/npc [.., x (24.8), y (24.8), facing]
        other     [.., +4 word]
     triggers: [[kind, vtable, x0, y0, x1, y1, test_vtable], ...] (point test: x0 <= px < x1, y0 <= py < y1)
  events: [frame, kind, ...]: ['sfx', id, vol] | ['sfx_stop', id] | ['song', id] | ['trigger', kind, vtable, x0, y0,
          x1, y1] (activation call) | ['event', 'sNNN'] (event started) | ['script', code_addr_hex, caller_hex]
"""
import fnmatch
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import emu, KEY, u32  # noqa: E402
from warp import warp  # noqa: E402

OUT = os.path.join(ROOT, 'data', 'field')
PLAYER = 0x03001C34
GD = 0x03001068
RNG = 0x03004A5C
FC = 0x03000F94
WS = 0x0300197C
WORLD = 0x03001C4C
TRIGS = 0x03002A64
SFX_TABLE = 0x083192A8
SONG_TABLE = 0x083BB78C
PLAY_SFX, STOP_SFX, PLAY_SONG = 0x0803E378, 0x0803E302, 0x0803F110
TRIG_ACTIVATE = 0x0801279C   # trigger check: r0 = test, r1 = trigger (about to call test->activate)
EVENT_START = 0x0801246E     # event runner ctor: r1 = event header
EVENT_RUN = 0x08012588       # run event (header r0)
EVENT_TABLE = 0x0879B704
INTERP = 0x08004038          # bytecode interpreter (r1 = code)

VT = {
    0x0804C9F0: 'player', 0x0804C484: 'enemy', 0x0804C92C: 'npc', 0x0804CEF4: 'npc_talk',
    0x0804CE00: 'breakable', 0x0804D1E4: 'prop', 0x08049CC4: 'coin', 0x0804B428: 'drop_item',
    0x0804A314: 'map_item', 0x08049FEC: 'hud', 0x0804E1B4: 'worldmap_player', 0x0804C83C: 'item_get',
}
TVT = {
    0x0804A09C: 'exit', 0x08049BC4: 'edge_exit', 0x08049BD4: 'event_trigger', 0x08049784: 'script_trigger',
    0x08049794: 'save_point', 0x0804B070: 'message_trigger', 0x080499D4: 'region', 0x0804A0C0: 'coin_pickup',
    0x08049D98: 'item_pickup',
}
STATE = {0x0804C790: 'idle', 0x0804A77C: 'idle', 0x0804C3A8: 'walk', 0x0804BEF0: 'dash', 0x0804C35C: 'punch',
         0x0804E00C: 'autowalk', 0x0804C384: 'bored', 0x0804D75C: 'hurt'}
EVENTS = {}


def ev_index():
    if not EVENTS:
        for i in range(522):
            EVENTS[u32(EVENT_TABLE + 4 * i)] = i
    return EVENTS


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def s16v(v):
    return v - 0x10000 if v & 0x8000 else v


def keymask(s):
    if isinstance(s, int):
        return s
    m = 0
    for k in s.split('+'):
        if k:
            m |= getattr(KEY, k)
    return m


def objects(e):
    n = e.r32(WORLD + 4)
    out = []
    for i in range(min(n, 128)):
        o = e.r32(WORLD + 8 + 4 * i)
        vt = e.r32(o)
        k = VT.get(vt, 'other')
        w = lambda off: s32v(e.r32(o + off))
        if k == 'breakable':
            ex = [w(0x14), w(0x18), w(0x1C), w(0x20), e.r8(o + 0xC), w(0x24), w(0x10)]
        elif k == 'coin':
            ex = [w(0x50), w(0x54), w(0x30), w(0x2C), w(0x24), w(0x14), w(0x18), w(0x1C), w(0x20)]
        elif k == 'drop_item':
            ex = [w(0x48), w(0x60), w(0x64), w(0x30), w(0x34), w(0x40), w(0x18), w(0x1C), w(0x20), w(0x24)]
        elif k == 'map_item':
            ex = [(e.r32(o + 0x14) - 0x083BD2F8) // 0x34, w(0x34), w(0x38), w(0x3C), w(0x40)]
        elif k in ('player', 'enemy', 'npc', 'npc_talk', 'worldmap_player'):
            ex = [w(0x1A0), w(0x1A4), e.r8(o + 0x10)]
        else:
            ex = [w(4)]
        out.append([k, f'{vt:08x}'] + ex)
    return out


def triggers(e):
    n = e.r32(TRIGS)
    out = []
    for i in range(min(n, 64)):
        t = e.r32(TRIGS + 4 + 4 * i)
        vt = e.r32(t)
        test = e.r32(t + 0xC)
        out.append([TVT.get(vt, 'other'), f'{vt:08x}'] + [s16v(e.r16(t + 4 + 2 * k)) for k in range(4)]
                   + [f'{e.r32(test):08x}' if test else None])
    return out


def player(e):
    p = e.r32(PLAYER)
    if not p:
        return None
    st = e.r32(p + 0x14)
    stv = e.r32(st) if st else 0
    return [s32v(e.r32(p + 0x1A0)), s32v(e.r32(p + 0x1A4)), e.r8(p + 0x10), STATE.get(stv, f'vt_{stv:08x}')]


def supplies(e):
    b = e.read(GD + 2, 83)
    return {i: b[i] for i in range(83) if b[i]}


def equipment(e):
    n = e.r32(GD + 0xE4)
    return [e.r32(GD + 0xE8 + 4 * i) for i in range(min(n, 250))]


def area(e):
    return [e.r8(WS), e.r8(WS + 1), e.r8(WS + 2)]


def record(sc, base_dir):
    e = emu(sc['state'])
    e.set_volume(0) if hasattr(e, 'set_volume') else None
    sc['setup'](e)
    ev_index()
    init = {
        'rng': [e.r32(RNG), e.r32(RNG + 4)],
        'player': dict(zip(['x', 'y', 'facing', 'state'], player(e))),
        'flags_hex': e.read(GD + 0x55, 143).hex(),
        'zenie': e.r32(GD + 0x4F4),
        'supplies': supplies(e),
        'equipment': equipment(e),
        'objs': objects(e),
        'triggers': triggers(e),
    }
    bps = {a: e.breakpoint(a) for a in (PLAY_SFX, STOP_SFX, PLAY_SONG, TRIG_ACTIVATE, EVENT_START, EVENT_RUN, INTERP)}
    frames, events = [], []
    sup0, trig0, zen0 = supplies(e), init['triggers'], init['zenie']
    f = 0
    fc = e.r32(FC)
    for keys, n in sc['input']:
        km = keymask(keys)
        for _ in range(n):
            e.set_keys(km)
            e.run(1)
            for h in e.hits():
                a, r = h['addr'], h['regs']
                if a == PLAY_SFX:
                    events.append([f, 'sfx', (r[1] - SFX_TABLE) // 12, r[2] & 0xFF])
                elif a == STOP_SFX:
                    events.append([f, 'sfx_stop', (r[1] - SFX_TABLE) // 12])
                elif a == PLAY_SONG:
                    events.append([f, 'song', r[1]])
                elif a == TRIG_ACTIVATE:
                    t = r[1]
                    vt = e.r32(t)
                    # a pickup trigger is destroyed inside its activation (vtable back to the base 0x0804E444 or reused
                    # memory): its rect is still in memory
                    kind = TVT.get(vt, 'destroyed_pickup')
                    events.append([f, 'trigger', kind, f'{vt:08x}'] + [s16v(e.r16(t + 4 + 2 * k)) for k in range(4)])
                elif a in (EVENT_START, EVENT_RUN):
                    hdr = r[1] if a == EVENT_START else r[0]
                    events.append([f, 'event', f's{EVENTS[hdr]:03d}' if hdr in EVENTS else f'0x{hdr:08x}'])
                elif a == INTERP:
                    events.append([f, 'script', f'0x{r[1]:08x}', f'0x{h["lr"]:08x}'])
            nfc = e.r32(FC)
            row = {'f': f, 'keys': km, 'logic': nfc != fc, 'rng': [e.r32(RNG), e.r32(RNG + 4)], 'p': player(e),
                   'area': area(e), 'objs': objects(e)}
            fc = nfc
            z = e.r32(GD + 0x4F4)
            if z != zen0:
                row['zenie'] = z
                zen0 = z
            sup = supplies(e)
            if sup != sup0:
                row['supplies_changed'] = {k: sup.get(k, 0) for k in set(sup) | set(sup0) if sup.get(k, 0) != sup0.get(k, 0)}
                sup0 = sup
            row['equipment_n'] = e.r32(GD + 0xE4)
            tr = triggers(e)
            if tr != trig0:
                row['triggers'] = tr
                trig0 = tr
            frames.append(row)
            f += 1
    for b in bps.values():
        e.clear(b)
    return {'name': sc['name'], 'source_state': sc['state'], 'notes': sc['notes'], 'area': frames[0]['area'],
            'format': 'tools/field/rec_objects.py', 'init': init, 'input': sc['input'], 'frames': frames,
            'events': events}


# --- scenario setups -------------------------------------------------------------------------------------------

def place(e, x, y, facing, settle=2):
    p = e.r32(PLAYER)
    e.w32(p + 0x1A0, x << 8)
    e.w32(p + 0x1A4, y << 8)
    e.w8(p + 0x10, facing)
    e.set_keys(0)
    e.run(settle)


def setup_breakable(e):
    assert warp(e, 8, 1, 280, 290)
    place(e, 240, 104, 1)


def setup_breakable_drops(e):
    """8-1 breakable #0 with its def copied to RAM and the on_break bytecode replaced by
    drop_item(5); drop_equipment(44) (push 5; native 0x85; push 44; native 0x86; end)"""
    setup_breakable(e)
    b = [o for o in range(e.r32(WORLD + 4)) if e.r32(e.r32(WORLD + 8 + 4 * o)) == 0x0804CE00]
    brk = e.r32(WORLD + 8 + 4 * b[0])
    d = e.r32(brk + 0x28)
    RAMDEF, CODE = 0x0203F800, 0x0203F820
    for i in range(0, 16, 4):
        e.w32(RAMDEF + i, u32(d + i))
    code = bytes([0x00, 0x05, 0x02, 0x85, 0x00, 44, 0x02, 0x86, 0x11])
    for i, c in enumerate(code):
        e.w8(CODE + i, c)
    e.w32(RAMDEF + 0xC, CODE)
    e.w32(brk + 0x28, RAMDEF)


def setup_item(e):
    assert warp(e, 8, 57, 112, 170)
    place(e, 112, 140, 1)


def setup_touch(e):
    pass


def setup_save(e):
    assert warp(e, 8, 1, 280, 290)
    place(e, 150, 170, 1)


def setup_capsule(e):
    assert warp(e, 8, 1, 280, 290)
    place(e, 280, 275, 1)


A_EVERY = lambda n, gap: sum([[['A', 1], ['', gap]] for _ in range(n)], [])

SCENARIOS = [
    {'name': 'breakable_8_1', 'state': 'buu_snakeway_start.ss', 'setup': setup_breakable,
     'notes': 'Southwest Forest 8-1, breakable #0 (prop 1, hp 2, attack mask 3, loot inline_0x0807AAF0 = s497); '
              'player below it facing up punches (A), the loot (coins) flies up; then the player walks up / around to '
              'collect the coins (touch).',
     'input': [['', 4]] + A_EVERY(4, 30) + [['', 40], ['UP', 50], ['LEFT', 20], ['RIGHT', 40], ['DOWN', 30], ['', 60]]},
    {'name': 'breakable_drops_8_1', 'state': 'buu_snakeway_start.ss', 'setup': setup_breakable_drops,
     'notes': 'as breakable_8_1 but the on_break bytecode (def copied to RAM 0x0203F800) is drop_item(5); '
              'drop_equipment(44): a supply and an equipment pickup fly up (4 RNG draws each), bounce and land; '
              'the player walks up and presses A (interact message 3) to pick them up (60-frame item-get each).',
     'input': [['', 4]] + A_EVERY(2, 30) + [['', 100], ['UP', 40], ['LEFT', 14], ['A', 1], ['', 80], ['A', 1], ['', 20],
                                            ['RIGHT', 22], ['UP', 4], ['A', 1], ['', 80], ['A', 1], ['', 40]]},
    {'name': 'item_8_57', 'state': 'buu_snakeway_start.ss', 'setup': setup_item,
     'notes': 'Inside Babidi\'s Ship 8-57, map item pickup (supply 37, flag_clear 69, event s333): the player below '
              'it facing up presses A (interact box up = (x, y-16)-(x+16, y+8) vs the item box), then A to close '
              'the text.',
     'input': [['', 4]] + A_EVERY(8, 40) + [['', 60]]},
    {'name': 'touch_trigger_1_1', 'state': 'buu_snakeway_start.ss', 'setup': setup_touch,
     'notes': 'Snakeway 1-1: walk down then right into the touch_once event trigger s039 (rect 361,153-384,232; '
              'fires when the player point x>>8 reaches 361); A presses advance the dialog.',
     'input': [['DOWN', 16], ['RIGHT', 170]] + A_EVERY(10, 30) + [['RIGHT', 60], ['', 30]]},
    {'name': 'save_point_8_1', 'state': 'buu_snakeway_start.ss', 'setup': setup_save,
     'notes': 'Southwest Forest 8-1 save point (rect 128,152-185,193, A button, event s011): A, then B/A in the menu.',
     'input': [['', 4], ['A', 1], ['', 90], ['B', 1], ['', 40], ['B', 1], ['', 40]]},
    {'name': 'a_trigger_capsule_8_1', 'state': 'buu_snakeway_start.ss', 'setup': setup_capsule,
     'notes': 'Southwest Forest 8-1 capsule script trigger (rect 265,249-312,280, A button, inline_0x0807AAAC -> '
              's515 -> world map 0-50).',
     'input': [['', 4], ['A', 1], ['', 200]]},
]


def main():
    globs = [a for a in sys.argv[1:] if not a.startswith('-')]
    os.makedirs(OUT, exist_ok=True)
    for sc in SCENARIOS:
        if globs and not any(fnmatch.fnmatch(sc['name'], g) for g in globs):
            continue
        tr = record(sc, OUT)
        path = os.path.join(OUT, f'objects_{sc["name"]}.json')
        with open(path, 'w') as fh:
            json.dump(tr, fh, separators=(',', ':'))
        ev = [x for x in tr['events'] if x[1] != 'script' or True]
        print(sc['name'], len(tr['frames']), 'frames', len(ev), 'events ->', os.path.relpath(path, ROOT))


if __name__ == '__main__':
    main()
