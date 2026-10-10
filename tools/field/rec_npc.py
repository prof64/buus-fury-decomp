"""rec_npc.py [NAME_GLOB ...]: record NPC ground truth (headless, muted) -> data/field/npc_<name>.json

Each scenario warps from buu_snakeway_start.ss into an area (re/buu/warp.py's exit-trigger trick: a normal area
transition), then records every video frame from the first game-logic frame after the area load:

  init.rng        RNG state when the area loader 0x08008B30 starts (before any object of the area exists)
  init.rng_f0     RNG state at the start of frame 0 (draws between the two: init.load_draws)
  init.flags      story flag bytes GameData+0x55.. (0x03001068 + 0x55, 0x90 bytes) as hex
  init.npcs       every NPC object in object-list order (list 0x03001C50 index), its ROM def, position (24.8),
                  facing, sprite, footprint, behaviour step index and the current step's raw words
  frames[f]       {f, logic, rng, draws, p, npcs}: logic = the frame counter 0x03000F94 advanced;
                  draws = RNG calls of the frame [[caller lr, 'rand'|'chance'|'next', arg]];
                  npcs = [def, x, y, facing, raw anim, anim frame] in init.npcs order (None once destroyed)
  input           [[keys, video frames], ...] applied from frame 0 (KEYINPUT bit order: A=1 B=2 ... R=0x100 L=0x200)
"""
import fnmatch
import json
import sys
from npc_lib import *

OUT = ROOT / 'data' / 'field'
AREAS = 0x0808E2E0
NPC_CTORS = {0x08017DA3: 0x14, 0x080111A9: 0x10, 0x0800FD55: 0x18}  # ctor -> def offset of the behaviour list
ENEMY_LIST = 0x03001BB0
RAND, CHANCE, NEXT = 0x0803FACC, 0x0803FADE, 0x0803FAAC

SCENARIOS = [
    {'name': 'yemma_idle', 'area': (1, 3), 'pos': (240, 416), 'input': [[0, 900]],
     'notes': "Yemma's Office: 4 wander_rect talk NPCs (speed 64), 2 wait-30 NPCs, stands; player idle"},
    {'name': 'nataday_idle', 'area': (15, 1), 'pos': (400, 640), 'input': [[0, 900]],
     'notes': 'Nataday Village: 4 wander talk NPCs (speed 128 / 64); player idle'},
    {'name': 'westcity_idle', 'area': (4, 1), 'pos': (440, 360), 'input': [[0, 900]],
     'notes': 'West City: 7 wander_rect talk NPCs (speed 128), wait-10 NPCs; player idle'},
    {'name': 'capsule_walk', 'area': (4, 20), 'pos': (240, 240), 'input': [[0, 1500]],
     'notes': 'Capsule Corporation: a talk NPC with a wait / walk_to loop (speed 128); player idle'},
    {'name': 'plain_wander', 'area': (4, 23), 'pos': None, 'input': [[0, 900]],
     'notes': 'plain (no talk) NPC wander 0x0800A7DF'},
    {'name': 'plain_wander_rect', 'area': (7, 3), 'pos': None, 'input': [[0, 900]],
     'notes': 'plain NPC wander_rect 0x0800A811 (chance x8)'},
    {'name': 'yemma_talk', 'area': (1, 3), 'pos': (200, 432),
     'input': [[0, 40], [KEY.UP, 40], [0, 10], [KEY.A, 2], [0, 150], [KEY.A, 2], [0, 90], [KEY.A, 2], [0, 90],
               [KEY.A, 2], [0, 90], [KEY.A, 2], [0, 90], [KEY.A, 2], [0, 200]],
     'notes': "Yemma's Office: walk up to the NPC at (184, 400) (event s053) and press A; then A to advance the text"},
    {'name': 'yemma_block', 'area': (1, 3), 'pos': (136, 424),
     'input': [[0, 40], [KEY.LEFT, 40], [0, 20], [KEY.UP, 30], [KEY.RIGHT, 30], [0, 300], [KEY.DOWN, 20], [KEY.LEFT, 60],
               [KEY.UP, 40], [0, 200], [KEY.RIGHT, 120], [0, 300]],
     'notes': "Yemma's Office: the player walks around inside the wander rect of the NPC at (128, 400) (blocking)"},
    {'name': 'capsule_block', 'area': (4, 20), 'pos': (152, 140), 'input': [[0, 1500]],
     'notes': 'Capsule Corporation: the player stands on the walk_to path of the NPC (walk_to blocked by the player)'},
    {'name': 'yemma_crowd', 'area': (1, 3), 'pos': (120, 290), 'input': [[0, 1200]],
     'notes': "Yemma's Office: the player stands inside the small wander rect of the NPC at (128, 280)"},
    {'name': 'enemy_patrol_far', 'area': (14, 9), 'pos': (1000, 1000), 'input': [[0, 1500]],
     'notes': 'Kyodai Pyramid 14-9 with the player parked off the map (1000, 1000): no enemy ever sees the player, the '
              'type-77 mummies patrol their walk_to loops (map behaviour via RunMotionController) the whole time'},
    {'name': 'enemy_patrol', 'area': (14, 9), 'pos': (0x40, 0x100), 'input': [[0, 900]],
     'notes': 'Kyodai Pyramid 14-9: type-77 enemies whose AI runs RunMotionController (0x080299A3): the map behaviour '
              '(def +0x18: walk_to 0x0800A1D5 loops); enemies recorded in init.enemies / frames[].enemies'},
]


AI_STATES = 0x0870F514


def ai_top(e, o):
    """AI stack at obj+0x238: [actor, count, {node, state ptr} * count]; top state id"""
    st = o + 0x238
    n = e.r32(st + 4)
    if not n or n > 16:
        return None
    return (e.r32(st + 8 * n + 4) - AI_STATES) // 8


def enemy_row(e, o):
    """[x, y, facing, runner idx, raw, anim frame, anim tick, anim state, AI top state, step vt, step word +4, +8, +0x10]"""
    cur = e.r32(o + 0x22C)
    idx = e.r8(o + 0x22C + 4)
    return [s32v(e.r32(o + 0x1A0)), s32v(e.r32(o + 0x1A4)), e.r8(o + 0x10), idx - 256 if idx >= 128 else idx,
            anim_raw(e, o), e.r8(o + 0x11), e.r16(o + 0x12), hex(e.r32(o + 0x14)), ai_top(e, o),
            STEP_VT.get(e.r32(cur), hex(e.r32(cur))) if cur else None,
            s32v(e.r32(cur + 4)) if cur else None, s32v(e.r32(cur + 8)) if cur else None,
            e.r16(cur + 0x10) if cur else None]


def enemy_objs(e):
    n = e.r32(ENEMY_LIST)
    return [e.r32(ENEMY_LIST + 4 + 4 * i) for i in range(min(n, 64))]


def area_defs(w, m):
    """actor defs of every area record with (world, map) (all variants)"""
    out = {}
    for i in range(0x1C4):
        a = AREAS + 0x38 * i
        if (u8(a), u8(a + 1)) != (w, m):
            continue
        for k in range(u8(a + 4)):
            d = u32(u32(a + 0x14) + 4 * k)
            c = u32(d)
            if c in NPC_CTORS:
                out[d + NPC_CTORS[c]] = d
    return out


def first_pos(w, m):
    import glob
    base = ROOT / 'work' / 'field_content' / 'maps'
    for f in sorted(glob.glob(str(base / f'area_{w:02d}_{m:03d}*' / 'objects.json'))):
        return tuple(json.load(open(f))['start_pos'])
    return (200, 200)


class Draws:
    def __init__(self, e):
        self.e = e
        self.ids = []

    def arm(self):
        self.ids = [self.e.breakpoint(a) for a in (RAND, CHANCE, NEXT)]

    def disarm(self):
        for b in self.ids:
            self.e.clear(b)
        self.ids = []

    def collect(self):
        out = []
        for h in self.e.hits():
            if h['kind'] != 'break':
                continue
            pc = h['pc'] & ~1
            r = h['regs']
            # the hit is taken before the instruction at pc executes: lr = caller, r0 = argument
            name = {RAND: 'rand', CHANCE: 'chance', NEXT: 'next'}.get(pc)
            if name is None:
                # pipeline offset: match the nearest entry
                for a, n in ((RAND, 'rand'), (CHANCE, 'chance'), (NEXT, 'next')):
                    if 0 <= pc - a <= 8:
                        name, pc = n, a
            if name == 'next' and RAND <= (r[14] & ~1) < CHANCE + 0x20:
                continue  # inner call of rand / chance
            d = [hex(r[14] & ~1), name, r[0] if name != 'next' else None]
            if (r[14] & ~1) == 0x0800DFC8:  # idle state enter (SetAnim(0)): r4 = the actor
                d.append(hex(r[4]))
                d.append(hex(self.e.r32(r[4])))
            elif (r[14] & ~1) in (0x0800A2CE, 0x0800A2E4):  # wander step: r5 = the actor
                d.append(hex(r[5]))
            out.append(d)
        return out


def goto_f0(scn):
    """warp into the scenario's area and stop at the start of frame 0: (emu, rng at the loader, pre-frame-0 draws)"""
    w, m = scn['area']
    pos = scn['pos'] or first_pos(w, m)
    e = emu('buu_snakeway_start.ss')
    fix = arm_warp(e, w, m, pos[0], pos[1])
    area0 = e.r32(AREA_PTR)
    load_rng = []

    def at_loader(e, pc):
        load_rng.append(rng(e))
    e.set_keys(KEY.RIGHT)
    for f in range(800):
        fix()
        emutools.catch(e, [LOADER], 1, keyfn=lambda _: KEY.RIGHT, cb=at_loader)
        if e.r32(AREA_PTR) != area0:
            break
    else:
        raise RuntimeError('warp failed')
    e.set_keys(0)
    # wait for the first game-logic frame of the new area
    D = Draws(e)
    pre = []
    for f in range(400):
        fc = e.r32(FRAME_COUNTER)
        st = e.save()
        had_player = e.r32(PLAYER) != 0
        D.arm()
        e.run(1)
        d = D.collect()
        D.disarm()
        if e.r32(FRAME_COUNTER) != fc and e.r32(PLAYER) and had_player:
            e.load(st)  # frame 0 starts here
            break
        pre.append(d)
    else:
        raise RuntimeError('no logic frame after the load')
    return e, load_rng, pre, D


def record(scn):
    w, m = scn['area']
    pos = scn['pos'] or first_pos(w, m)
    e, load_rng, pre, D = goto_f0(scn)
    defs = area_defs(w, m)
    objs = npc_objects(e)

    def npc_info(o):
        lst = e.r32(o + 0x1B0 + 8)
        d = defs.get(lst)
        st = step_state(e, o)
        return {'obj': hex(o), 'def': hex(d) if d else None, 'ctor': hex(u32(d)) if d else None,
                'talk_event': hex(e.r32(o + 0x1D0)) if e.r32(o) == VT_TALK_NPC else None,
                'x': s32v(e.r32(o + 0x1A0)), 'y': s32v(e.r32(o + 0x1A4)), 'facing': e.r8(o + 0x10),
                'foot': [e.r8(o + 0x19C), e.r8(o + 0x19D)], 'raw': anim_raw(e, o), 'anim_frame': e.r8(o + 0x11),
                'list_index': None, 'step_index': st['index'], 'step_state': st}
    lst = 0x03001C50
    order = [e.r32(lst + 4 + 4 * k) for k in range(e.r32(lst))]
    init = {
        'rng': load_rng[-1] if load_rng else None,
        'rng_f0': rng(e),
        'load_draws': [x for d in pre for x in d],
        'lag_before_f0': len(pre),
        'frame0': 'first video frame after the area load on which the frame counter 0x03000F94 advances; '
                  'init values are read at the start of that frame (before its logic)',
        'frame_counter': e.r32(FRAME_COUNTER),
        'flags': e.read(GAMEDATA + 0x55, 0x90).hex(),
        'player': dict(zip(['x', 'y', 'facing'], player_row(e))) | {'list_index': order.index(e.r32(PLAYER)) if e.r32(PLAYER) in order else None},
        'npcs': [],
        'object_list': [[hex(o), hex(e.r32(o))] for o in order],
    }
    init['enemies'] = []
    for o in enemy_objs(e):
        d = e.r32(o + 0x22C + 8) - 0x18
        st = e.r32(o + 0x22C)
        init['enemies'].append({'obj': hex(o), 'def': hex(d), 'type': u32(d + 8) if u32(d) == 0x0800FD55 else None,
                                'x': s32v(e.r32(o + 0x1A0)), 'y': s32v(e.r32(o + 0x1A4)), 'facing': e.r8(o + 0x10),
                                'foot': [e.r8(o + 0x19C), e.r8(o + 0x19D)], 'no_grid': e.r8(o + 0x291),
                                'list_index': order.index(o) if o in order else None,
                                'runner_index': e.r8(o + 0x22C + 4), 'row0': enemy_row(e, o)})
    for o in objs:
        info = npc_info(o)
        info['list_index'] = order.index(o) if o in order else None
        init['npcs'].append(info)
    keys = []
    for k, n in scn['input']:
        keys += [k] * n
    frames = []
    for f, k in enumerate(keys):
        e.set_keys(k)
        fc = e.r32(FRAME_COUNTER)
        D.arm()
        e.run(1)
        d = D.collect()
        D.disarm()
        live = set(npc_objects(e))
        row = {'f': f, 'logic': e.r32(FRAME_COUNTER) != fc, 'rng': rng(e), 'draws': d, 'p': player_row(e),
               'npcs': [npc_row(e, int(n['obj'], 16))[1:] if int(n['obj'], 16) in live else None for n in init['npcs']],
               'talk_npc': hex(e.r32(0x03001BAC)) if e.r32(0x03001BAC) else None}
        if init['enemies']:
            live_e = set(enemy_objs(e))
            row['enemies'] = [enemy_row(e, int(x['obj'], 16)) if int(x['obj'], 16) in live_e else None
                              for x in init['enemies']]
        frames.append(row)
    e.set_keys(0)
    return {'name': scn['name'], 'source_state': 'work/states/buu_snakeway_start.ss', 'notes': scn['notes'],
            'area': [w, m, e.r8(0x0300197E)], 'warp_to': list(pos),
            'init': init, 'input': scn['input'], 'frames': frames}


def main():
    pats = sys.argv[1:] or ['*']
    OUT.mkdir(parents=True, exist_ok=True)
    for scn in SCENARIOS:
        if not any(fnmatch.fnmatch(scn['name'], p) for p in pats):
            continue
        t = record(scn)
        p = OUT / f"npc_{scn['name']}.json"
        p.write_text(json.dumps(t, separators=(',', ':')))
        moved = sum(1 for i in range(len(t['init']['npcs'])) if len({tuple(r['npcs'][i][:2]) for r in t['frames'] if r['npcs'][i]}) > 1)
        print(scn['name'], 'npcs', len(t['init']['npcs']), 'moving', moved, 'frames', len(t['frames']),
              'lag', sum(1 for r in t['frames'] if not r['logic']), '->', p.relative_to(ROOT))


if __name__ == '__main__':
    main()
