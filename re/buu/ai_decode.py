"""Enemy AI (behaviour tree) node registry + decoder for Dragon Ball Z: Buu's Fury.

Data model
----------
* State table 0x0870F514: 8-byte entries {behaviour-tree root def*, aux*}, indexed by AI state id.
* Enemy type table 0x08711274 (28 B / type): +8 = initial AI state id.
* Enemy object keeps an AI stack at obj+0x238; enemy vfuncs +0x68 replace top state (0x0800AA4A),
  +0x6C push (0x0800AA9C), +0x70 pop (0x0800AAD8).  GotoState nodes call +0x68.
* A node *definition* is a ROM word array [create_fn, f1, f2, params..., children...].  create_fn
  (thumb) either allocates a runtime object whose vtable literal is in the create fn, or returns a
  shared IWRAM singleton (vtable lives in RAM, set at boot).  Runtime vtable slots: [0] enter(obj),
  [1] update(obj) -> 0 running / non-zero finished, [2] exit(obj), [5] condition(obj) for the
  shared "conditional decorator" (enter 0x080342A9, update 0x080342AF, exit 0x08034307): while the
  condition is true the child is entered/updated and its result returned; when false the child is
  exited and 0 is returned.
* "Controller actions": many action nodes switch the enemy's motion controller (obj+0x14) with
  0x0800ED18 to an IWRAM singleton (e.g. 0x03002FC4 gun shot) and report done once obj+0x14 is no
  longer that controller.  Controller update fns count frames in obj+0x12 (u16), anim frame in
  obj+0x11; they set an anim with 0x0800ECDA(obj, n), spawn objects with 0x0801191A(world, obj)
  and play sounds with 0x0803E378.

Units: distances in the conditions are squared pixels (centre to centre); speeds are 1/256 px per
frame; chances are rand16 < p, i.e. p/65536 per frame; times are frames (60 fps).
"""
import ai2_boot  # noqa: F401
import math
import re
import capstone
from common import *
from vtab import entries
from ai_nodes import analyze

STATE_TABLE = 0x0870F514
ENEMY_TABLE = 0x08711274
ENEMY_SIZE = 28
N_ENEMIES = 0x96

COND_ENTER, COND_UPDATE, COND_EXIT = 0x080342A9, 0x080342AF, 0x08034307
SET_ANIM, SET_CTRL, ADD_TO_WORLD, PLAY_SOUND, RAND = 0x0800ECDA, 0x0800ED18, 0x0801191A, 0x0803E378, 0x0803FACC
POOL_ROOM = 0x080116FC   # free capacity of an object pool (<<6); spawners require >= 0x80 / 0x400
IDLE_CTRL = 0x03001B30
FACE_EVENT_VT = 0x0804AE70

md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)

# ---------------------------------------------------------------------------------------------
# hand-decoded registry.  params: (offset, name, unit).  children: 'list' (count @+0xC, defs from
# +0x10) or a list of offsets.  status: VERIFIED (emulator) / DISASM (read from code) / INFERRED.
# ---------------------------------------------------------------------------------------------
L = 'list'
REGISTRY = {
    # ---- composites ----
    0x08018F91: dict(name='Sequence', kind='composite', children=L, status='DISASM',
                     sem='run children one at a time in order; finished when the last child finishes'),
    0x080191C7: dict(name='ParallelAll', kind='composite', children=L, status='DISASM',
                     sem='update every child each frame; finished when all children have finished'),
    0x080191E9: dict(name='ParallelAny', kind='composite', children=L, status='DISASM',
                     sem='update every child each frame; finished as soon as any child finishes'),
    0x0801920F: dict(name='Selector', kind='composite', children=L, status='DISASM',
                     sem='each frame update children in order until one returns non-zero (then return 1); '
                         'typically a list of guarded decorators = priority list'),
    # ---- state transitions ----
    0x0801890B: dict(name='GotoState', kind='transition', params=[(0xC, 'state', 'state')], status='DISASM',
                     sem='replace the current AI state (enemy vfunc +0x68 -> 0x0800AA4A); returns 1'),
    0x08019285: dict(name='SubState', kind='inline', params=[(0xC, 'state', 'state')], status='DISASM',
                     sem="run state <state>'s behaviour tree in place (a reusable sub-tree)"),
    # ---- conditional decorators (child @+0x10 unless noted) ----
    0x080188A9: dict(name='Chance', kind='cond', params=[(0xC, 'p', 'prob16')], children=[0x10], status='DISASM',
                     sem='each frame: rand16 < p -> run child (p/65536 per frame)'),
    0x08029661: dict(name='IfPlayerFarther', kind='cond', params=[(0xC, 'r2', 'dist2')], children=[0x10],
                     status='DISASM', sem='player centre further than sqrt(r2) px'),
    0x08029C65: dict(name='IfPlayerWithin', kind='cond', params=[(0xC, 'r2', 'dist2')], children=[0x10],
                     status='DISASM', sem='player centre within sqrt(r2) px (aggro radius)'),
    0x08029D15: dict(name='IfPlayerWithin2', kind='cond', params=[(0xC, 'r2', 'dist2')], children=[0x10],
                     status='DISASM', sem='player within sqrt(r2) px (variant using bbox centres)'),
    0x08039855: dict(name='IfTimeInState', kind='cond', params=[(0xC, 'frames', 'frames')], children=[0x10],
                     status='DISASM', sem='frames since obj+0x210 stamp (state entry / end of block) > frames'),
    0x0802AA3D: dict(name='IfCooldownOver', kind='cond', params=[(0xC, 'frames', 'frames')], children=[0x10],
                     status='DISASM', sem='now - obj+0x1E8 (set by MarkTime) > frames'),
    0x08029E7D: dict(name='IfCounterEq', kind='cond', params=[(0xC, 'n', 'count')], children=[0x10],
                     status='DISASM', sem='obj+0x1E4 counter == n'),
    0x0802AAD1: dict(name='IfGlobalFlag', kind='cond', children=[0xC], status='DISASM',
                     sem='global [0x03002FE4] > 0 (arena/boss event flag)'),
    0x080299DD: dict(name='IfHpBelow', kind='cond', params=[(0xC, 'frac', 'frac16')], children=[0x10],
                     status='DISASM', sem='on enter threshold = frac*maxHP>>16; true while HP (obj+0x28C) < threshold'),
    0x08029BB7: dict(name='IfPlayerAttackingMe', kind='cond', children=[0xC], status='DISASM',
                     sem='player in an attack state (0x03001B50) and player bbox (+8 px) overlaps the enemy'),
    0x08029B31: dict(name='IfInPlayerBeamLine', kind='cond', children=[0xC], status='DISASM',
                     sem="player faces the enemy, player state vfunc[0x1C] true (charging/firing) and the enemy "
                         "is in a 240 px corridor (+-4 px) in front of the player -> used to dodge beams"),
    0x0802692D: dict(name='IfAxisAligned', kind='cond', children=[0xC], status='DISASM',
                     sem='player attackable (+0x1CC) and |dx| < 16 or |dy| < 16 px (lined up for a shot)'),
    0x08029FE7: dict(name='IfPlayerInMeleeRange', kind='cond', children=[0xC], status='DISASM',
                     sem='player attackable and player bbox touches the enemy bbox grown by ~3-6 px; '
                         'turns the enemy (obj+0x10) toward the player on the dominant axis'),
    0x0803D479: dict(name='IfPlayerInFront', kind='cond', children=[0xC], status='INFERRED',
                     sem='player attackable and inside a box in front of the enemy (sets facing)'),
    0x08037A05: dict(name='IfPlayerInZone', kind='cond', children=[0xC], status='DISASM',
                     sem='player bbox inside the area rectangle at [[0x0876BA3C]+0x18] (sniper zone)'),
    0x08033F05: dict(name='IfPlayerInRoomZone', kind='cond', children=[0xC], status='INFERRED',
                     sem='player inside the trigger region of global object [0x087593C8]+0x80 (vampire lair)'),
    # ---- leaf actions ----
    0x08018C09: dict(name='Wait', kind='action', params=[(0xC, 'frames', 'frames')], status='DISASM',
                     sem='do nothing for <frames> frames, then finish'),
    0x0801D09D: dict(name='Succeed', kind='action', status='DISASM', sem='returns 1 immediately (singleton 0x03002BD0)'),
    0x0800A9B1: dict(name='Idle', kind='action', status='DISASM', sem='returns 0 forever (singleton 0x03001AF4)'),
    0x08019269: dict(name='StandStill', kind='action', status='DISASM',
                     sem='switch to the idle anim/state 0 (singleton 0x03002BA8); returns 1'),
    0x08018837: dict(name='Block', kind='action', status='DISASM',
                     sem='guard pose (anim state 7) with obj+0x1C8=1 (blocking) while running; exit clears it and stamps obj+0x210'),
    0x0802AA1B: dict(name='MarkTime', kind='action', status='DISASM', sem='obj+0x1E8 = frame counter (starts a cooldown)'),
    0x08029EC1: dict(name='IncCounter', kind='action', status='DISASM', sem='obj+0x1E4 += 1; returns 1'),
    0x0802968D: dict(name='SetCounter', kind='action', params=[(0xC, 'n', 'count')], status='DISASM',
                     sem='obj+0x1E4 = n'),
    0x0801D047: dict(name='FacePlayer', kind='action', status='INFERRED',
                     sem='sends event vt 0x0804AE70 {player} to the enemy (obj+0x18C) -> turn toward player; returns 1'),
    0x0802A09F: dict(name='ChasePlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='DISASM',
                     sem='path-find toward the player (0x0803BB00/0x0803BB48, re-path every 8 frames); stops '
                         '(state 0, returns 0) within ~2 px; footstep sounds for some gfx ids'),
    0x08018E29: dict(name='StepTowardPlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='DISASM',
                     sem='face the player and walk straight at it (walk anim state 1)'),
    0x0801CDA1: dict(name='StepAwayFromPlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='DISASM',
                     sem='walk directly away from the player (keep distance / flee)'),
    0x0801CFDB: dict(name='AlignWithPlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='INFERRED',
                     sem='strafe along one axis to line up with the player; returns 1 within 3 px'),
    0x0802B7C7: dict(name='Wander', kind='action', params=[(0xC, 'turn_p', 'prob16'), (0x10, 'speed', 'speed256')],
                     status='DISASM', sem='walk in a random direction (0..3, table 0x08752EB4); each frame '
                                           'rand16 < turn_p picks a new direction'),
    0x0802A289: dict(name='MeleeAttack', kind='action', status='VERIFIED',
                     sem='punch/kick: anim 0..3 chosen from the gfx def (+0x64/+0x68/+0x6C); hit box in front built '
                         'from attack data 0x086B8098 (melee, mult 0x400 = x1.0 of the enemy STR formula); finishes '
                         'when the anim ends'),
    0x0802B89D: dict(name='KiBlast', kind='action', status='DISASM',
                     sem='controller 0x03003068: anim state 3; frame 16 fires projectile 0x08019858 (3 px/frame, '
                         'dir table 0x08719C40); frame 32 back to idle'),
    0x080299A3: dict(name='RunMotionController', kind='action', status='INFERRED',
                     sem='ticks the controller object at obj+0x22C forever (never finishes)'),
    0x08033BB3: dict(name='VampireSummon', kind='action', status='DISASM',
                     sem='if the pool [0x08759360] has room spawn data 0x086FD9D0 at the enemy centre (bats / '
                         'minions, random 0x1000..0x1FFF param); exit restores the gfx anim; finishes immediately'),
    0x0803396F: dict(name='VampireSummon2', kind='action', status='DISASM',
                     sem='like VampireSummon with pool [0x08759320]'),
}

REGISTRY.update({
    0x08018877: dict(name='PushState', kind='transition', params=[(0xC, 'state', 'state')], status='DISASM',
                     sem='push AI state <state> on the enemy AI stack (vfunc +0x6C); returns 0'),
    0x0802D091: dict(name='WalkToPlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='DISASM',
                     sem='Spopovich: straight-line move toward the player at speed (normalised vector, collision '
                         'checked); faces the player on the dominant axis; finished within 8 px'),
    0x08035051: dict(name='FlyToPlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='INFERRED',
                     sem='vampires: normalised move toward the player ignoring walls (same maths as WalkToPlayer)'),
    0x080265E9: dict(name='GunShot', kind='action', status='DISASM',
                     sem='face player; controller 0x03002FC4: anim 0x12, frame 4 spawns a bullet (data 0x0874CA38, '
                         'builder 0x080274B4, sound 0x083193F8) if the pool has room; frame 16 back to idle'),
    0x08026869: dict(name='RifleShot', kind='action', status='DISASM',
                     sem='face player; controller 0x03002FD8: anim 0x12, frame 8 spawns a bullet (data 0x0874CA9C); '
                         'frame 32 back to idle'),
    0x08026739: dict(name='SoldierThrow', kind='action', status='INFERRED',
                     sem='face player; controller 0x03002FCC: anim 0x13, spawns projectile data 0x0874CA6C '
                         '(frame thresholds 2/8/32)'),
    0x0802E037: dict(name='GhostAnimA', kind='action', status='INFERRED',
                     sem='ghosts: anim 0x12 then a gfx-specific one-shot anim (0x60->0x5E, 0xDC->0xDB, 0xE5->0xE4) '
                         '-- vanish/appear'),
    0x0802E09F: dict(name='GhostAnimB', kind='action', status='INFERRED', sem='ghosts: counterpart of GhostAnimA'),
    0x0802F21D: dict(name='SpawnPuff', kind='action', status='INFERRED',
                     sem='enter: one-shot visual effect at the enemy centre (0x0800F924, flag obj+0x291); returns 1'),
})

for _fn in (0x08029D15, 0x08029C65, 0x0802A09F, 0x0802B7C7, 0x0802D091, 0x0802A289, 0x08026869, 0x080265E9,
            0x08018C09, 0x0801890B):
    REGISTRY[_fn]['status'] = 'VERIFIED'

UNIT_DESC = {
    'dist2': 'squared px (radius = sqrt)', 'frames': 'frames (1/60 s)', 'prob16': 'probability p/65536 per frame',
    'speed256': '1/256 px per frame', 'frac16': 'fraction p/65536', 'count': 'integer', 'state': 'AI state id',
    'px': 'pixels', 'enemy_type': 'enemy type index (0x08711274)', 'raw': 'raw value (meaning in sem)',
}

# boss / special node registries contributed by boss_nodes_*.py (same entry format as above)
import importlib  # noqa: E402
BOSS_GROUPS = ('buu', 'janemba', 'fighters', 'others', 'regular')
for _g in BOSS_GROUPS:
    try:
        REGISTRY.update(importlib.import_module(f'boss_nodes_{_g}').REGISTRY)
    except ImportError:
        pass


def convert(unit, v):
    if unit == 'dist2':
        return dict(px=round(math.sqrt(v), 2))
    if unit == 'frames':
        return dict(frames=v, seconds=round(v / 60, 3))
    if unit == 'prob16':
        return dict(percent_per_frame=round(v * 100 / 65536, 3),
                    mean_frames=round(65536 / v, 1) if v else None)
    if unit == 'speed256':
        return dict(px_per_frame=round(v / 256, 4))
    if unit == 'frac16':
        return dict(percent=round(v * 100 / 65536, 2))
    return dict(value=v)


# ---------------------------------------------------------------------------------------------
# automatic analysis (for nodes not in the registry)
# ---------------------------------------------------------------------------------------------
_emu = None
_auto = {}


def emu_ram():
    global _emu
    if _emu is None:
        _emu = emu('buu_fight_swforest.ss')
    return _emu


def insns(fn, n=200):
    fn &= ~1
    out = []
    seen_ret = False
    for ins in md.disasm(rom()[ra(fn):ra(fn) + n * 2], fn):
        if out and seen_ret and ins.mnemonic == 'push' and 'lr' in ins.op_str:
            break
        out.append(ins)
        if (ins.mnemonic == 'pop' and 'pc' in ins.op_str) or (ins.mnemonic == 'bx' and ins.op_str == 'lr'):
            seen_ret = True
        if len(out) > n:
            break
    return out


def lit(ins):
    if ins.mnemonic == 'ldr' and 'pc' in ins.op_str:
        off = int(ins.op_str.split('#')[1].rstrip(']'), 16)
        return u32(((ins.address + 4) & ~3) + off)
    return None


def bl_target(ins):
    if ins.mnemonic == 'bl':
        return int(ins.op_str.lstrip('#'), 16)
    return None


def runtime_vtable(fn):
    """(vtable, 'alloc'|'singleton', singleton_addr)."""
    ins = insns(fn, 30)
    lits = [lit(i) for i in ins if lit(i) is not None]
    vts = [x for x in lits if 0x08049000 <= x < 0x0804F000]
    if vts:
        return vts[-1], 'alloc', None
    io = [x for x in lits if 0x03000000 <= x < 0x03008000]
    if io:
        add = 0
        for i in ins[:4]:
            if i.mnemonic == 'bx':
                break
            if i.mnemonic == 'adds' and i.op_str.startswith('r0, #'):
                add = int(i.op_str.split('#')[1], 16)
        a = io[0] + add
        return emu_ram().r32(a), 'singleton', a
    return None, None, None


def fn_features(fn):
    """Calls / literals / small constants of one function."""
    ins = insns(fn)
    f = dict(anims=[], frame_cmps=[], spawns=0, sounds=[], data=[], ctrls=[], rand=False, face=False)
    prev = []
    for i in ins:
        t = bl_target(i)
        if t == SET_ANIM:
            for p in reversed(prev[-4:]):
                if p.mnemonic == 'movs' and p.op_str.startswith('r1, #'):
                    f['anims'].append(int(p.op_str.split('#')[1], 16))
                    break
        if t == SET_CTRL:
            for p in reversed(prev[-6:]):
                v = lit(p)
                if v is not None and p.op_str.startswith('r1') and 0x03000000 <= v < 0x03008000:
                    f['ctrls'].append(v)
                    break
        if t == ADD_TO_WORLD:
            f['spawns'] += 1
        if t == PLAY_SOUND:
            for p in reversed(prev[-6:]):
                v = lit(p)
                if v is not None and p.op_str.startswith('r1'):
                    f['sounds'].append(hex(v))
                    break
        if t == RAND:
            f['rand'] = True
        if i.mnemonic == 'cmp' and i.op_str.startswith('r0, #') and not (prev and bl_target(prev[-1]) == POOL_ROOM):
            f['frame_cmps'].append(int(i.op_str.split('#')[1], 16))
        v = lit(i)
        if v == FACE_EVENT_VT:
            f['face'] = True
        if v is not None and 0x086B0000 <= v < 0x08780000:
            f['data'].append(hex(v))
        prev.append(i)
    return f


def controller_summary(ctrl):
    vt = emu_ram().r32(ctrl)
    if not (0x08000000 <= vt < 0x08800000):
        return dict(controller=hex(ctrl))
    e = entries(vt, 3)
    en, up = fn_features(e[0]), fn_features(e[1])
    return dict(controller=hex(ctrl), vtable=hex(vt), anims=en['anims'] + up['anims'],
                frame_thresholds=sorted(set(up['frame_cmps'])), spawns_objects=up['spawns'] > 0,
                sounds=up['sounds'], data=sorted(set(en['data'] + up['data'])))


def auto(fn):
    if fn in _auto:
        return _auto[fn]
    vt, how, single = runtime_vtable(fn)
    a = dict(vtable=hex(vt) if vt else None, alloc=how, singleton=hex(single) if single else None)
    layout = analyze(fn)
    a['def_reads'] = {hex(k): v for k, v in sorted(layout.items())}
    if vt and 0x08000000 <= vt < 0x08800000:
        e = entries(vt, 6)
        if e[0] == COND_ENTER and e[1] == COND_UPDATE:
            a['kind'] = 'cond'
            c = fn_features(e[5])
            a['condition'] = dict(fn=hex(e[5]), data=c['data'], uses_rand=c['rand'])
        else:
            a['kind'] = 'action'
            en, up = fn_features(e[0]), fn_features(e[1])
            a['enter'], a['update'] = hex(e[0]), hex(e[1])
            a['faces_player'] = en['face'] or up['face']
            a['anims'] = en['anims'] + up['anims']
            a['spawns_objects'] = (en['spawns'] + up['spawns']) > 0
            a['uses_rand'] = en['rand'] or up['rand']
            a['data'] = sorted(set(en['data'] + up['data']))
            ctrls = sorted(set(en['ctrls'] + up['ctrls']))
            if ctrls:
                a['controllers'] = [controller_summary(c) for c in ctrls]
    _auto[fn] = a
    return a


def child_offsets(fn, d):
    r = REGISTRY.get(fn)
    if r is not None:
        ch = r.get('children')
        if ch == L:
            n = u32(d + 0xC)
            return [0x10 + 4 * k for k in range(min(n, 40))]
        return list(ch or [])
    lay = analyze(fn)
    return sorted(o for o, k in lay.items() if k == 'child' and o >= 0xC)


def def_size(fn, d):
    r = REGISTRY.get(fn)
    offs = [0x8]
    if r:
        offs += [o for o, *_ in r.get('params', [])] + child_offsets(fn, d)
    else:
        offs += list(analyze(fn).keys())
    return max(offs) + 4


def is_def(d):
    return 0x08000000 <= d < 0x08800000 and 0x08000000 <= u32(d) < 0x08100000 and u32(d) & 1


# ---------------------------------------------------------------------------------------------
# decoding
# ---------------------------------------------------------------------------------------------
def state_root(s):
    return u32(STATE_TABLE + 8 * s)


def initial_state(t):
    return u32(ENEMY_TABLE + ENEMY_SIZE * t + 8)


def decode(d, _depth=0):
    """Decode a node definition into a JSON-able dict."""
    if not is_def(d) or _depth > 40:
        return dict(addr=hex(d), invalid=True)
    fn = u32(d)
    r = REGISTRY.get(fn)
    node = dict(addr=hex(d), fn=hex(fn), name=r['name'] if r else f'node_{fn:08x}',
                kind=r['kind'] if r else auto(fn).get('kind', '?'))
    if r:
        ps = {}
        for off, name, unit in r.get('params', []):
            v = u32(d + off)
            ps[name] = dict(raw=v, unit=unit, **convert(unit, v))
        if ps:
            node['params'] = ps
    else:
        node['raw_words'] = [hex(u32(d + 4 * k)) for k in range(def_size(fn, d) // 4)]
    kids = [u32(d + o) for o in child_offsets(fn, d)]
    kids = [k for k in kids if is_def(k)]
    if kids:
        node['children'] = [decode(k, _depth + 1) for k in kids]
    return node


def describe(node):
    """One-line text for a decoded node."""
    s = node['name']
    if 'params' in node:
        bits = []
        for k, p in node['params'].items():
            if p['unit'] == 'dist2':
                bits.append(f"{k}={p['px']}px")
            elif p['unit'] == 'frames':
                bits.append(f"{p['frames']}f")
            elif p['unit'] == 'prob16':
                bits.append(f"{p['percent_per_frame']}%/f")
            elif p['unit'] == 'speed256':
                bits.append(f"{p['px_per_frame']}px/f")
            elif p['unit'] == 'frac16':
                bits.append(f"{p['percent']}%")
            elif p['unit'] == 'state':
                bits.append(f"state {p['raw']:#x}")
            else:
                bits.append(f"{k}={p['raw']}")
        s += '(' + ', '.join(bits) + ')'
    elif 'raw_words' in node:
        s += ' [' + ' '.join(node['raw_words'][1:]) + ']'
    return s


def pretty(node, indent=0, out=None):
    out = [] if out is None else out
    out.append('  ' * indent + describe(node) + f"  @{node['addr']}")
    for c in node.get('children', []):
        pretty(c, indent + 1, out)
    return out


def walk_states(start):
    """All states reachable from <start> via GotoState / SubState (BFS order)."""
    seen, order, todo = set(), [], [start]
    while todo:
        s = todo.pop(0)
        if s in seen or s > 0x400:
            continue
        seen.add(s)
        order.append(s)
        stack = [state_root(s)]
        vis = set()
        while stack:
            d = stack.pop()
            if d in vis or not is_def(d):
                continue
            vis.add(d)
            fn = u32(d)
            if fn in (0x0801890B, 0x08019285):
                todo.append(u32(d + 0xC))
                continue
            stack += [u32(d + o) for o in child_offsets(fn, d)]
    return order


if __name__ == '__main__':
    import sys
    for a in sys.argv[1:]:
        v = int(a, 16)
        if v < 0x1000:
            print(f'## state {v:#x}')
            print('\n'.join(pretty(decode(state_root(v)))))
        else:
            print('\n'.join(pretty(decode(v))))
