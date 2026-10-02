"""Decode the Buu's Fury AI data the battle simulator needs beyond data/buu/enemy_ai.json:

  aux        : the on-hit ("aux") tree of every AI state (state table 0x0870F514 word +4), evaluated through each
               node's onHit function (node def word +4) when the enemy is hit
  reactions  : behaviour trees of the generic reaction states 0x00..0x11 (knockback, Burning-Attack stun, flame
               knockback, volleyball, ...) that the aux trees push
  death      : the enemy-type +0x14 node, whose word +8 function runs when HP reaches 0 (0x0800FA2A)

onHit return protocol (disassembly, this script's comments): 0 = not handled, 1 = handled, 2 = stop the
surrounding Sequence. Sequence 0x0801900B: runs children until one returns 2, result = AND of the results;
Selector 0x08018FDB: first non-zero child result; ApplyDamage 0x080187A5: hit mult 0 -> 0, else damage
(vfunc +0x58) -> 2 if it killed the enemy else 1.

usage: .venv/bin/python tools/battle/gen_buu_ai_extra.py   -> data/battle/buu_ai_extra.json
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/buu'))
from ai_decode import REGISTRY, STATE_TABLE, ENEMY_TABLE, ENEMY_SIZE, N_ENEMIES, child_offsets, convert, is_def  # noqa: E402
from common import u32  # noqa: E402

# nodes whose create fn is not in the registry (reaction states) - names from their code (DISASM)
EXTRA_CREATE = {
    0x08018E5F: dict(name='PopState', sem='pop the AI stack (vfunc +0x70); onHit pops and returns 2'),
    0x080299C5: dict(name='StunHold', sem='hold controller 0x03002FF0 (stunned, no movement) - Burning Attack stun'),
    0x08029E5D: dict(name='DeathBlink', params=[(0xC, 'frames', 'frames')], sem='death blink for <frames> frames (rand flicker)'),
    0x08039BFD: dict(name='ShockStun', sem='electric shock stun (Electric Kamehameha reaction) until the anim ends'),
    0x0803C8BF: dict(name='VolleyballTransform', sem='turn into a volleyball (Ultra Volleyball Attack)'),
    0x0803C7FF: dict(name='VolleyballRestore', params=[(0xC, 'launched', 'raw')], sem='turn back from a volleyball; launched=1 when it was punched (flies, hits other enemies with melee 0x086B8098)'),
    0x0803C72F: dict(name='VolleyballLaunch', sem='the ball was punched: fly along the hit direction'),
    # rare nodes of states reached only through aux trees (automatic analysis: anim + controller only) [INFERRED]
    0x0802AE19: dict(name='PlayAnimHold', params=[], sem='anim 21 on hold controller 0x0300301C (never finishes) [INFERRED]', anim=21),
    0x0802AEF9: dict(name='PlayAnimTimed', params=[], sem='anim 20, controller 0x03003030 ends at frame 32 [INFERRED]', anim=20, frames=32),
    0x08033BBD: dict(name='PlayAnimOnce', params=[], sem='anim 17 played once (controller 0x03001B38) [INFERRED]', anim=17),
    0x08033D7D: dict(name='PlayAnimOnce', params=[], sem='anim 18 played once (controller 0x03001B38) [INFERRED]', anim=18),
    0x08034AA5: dict(name='PlayAnimTimed', params=[], sem='anims 20 then 18, controller 0x03003234 ends at frame 36 [INFERRED]', anim=20, frames=36),
    0x08036329: dict(name='IfMeleeHit', children=[0xC], sem='melee hit (0x086B8098) in front -> child (variant of 0x08036303) [INFERRED]'),
    0x08037C07: dict(name='RandomStep', sem='random-direction step (uses rand) [INFERRED]'),
    0x0803CE71: dict(name='IfUnknown', children=[0xC], sem='condition 0x0804EE25 not decoded: treated as false [INFERRED]'),
}
# onHit-only nodes (create fn 0): name by the onHit function
AUX_ONLY = {
    0x08029A41: dict(name='IfAttack', params=[(0xC, 'atk', 'raw')], children=[0x10], sem='hit attack type byte0 or byte1 == atk -> child'),
    0x080187A5: dict(name='ApplyDamage', sem='hit mult 0 -> 0; else take damage: 2 if it killed the enemy, else 1'),
    0x0803999D: dict(name='JanembaPartFlash', sem='Janemba body: flash the linked part (cosmetic); 0'),
    0x0803044D: dict(name='FreezeHelperHit', sem='freeze helper 0x0E: message 5 to the world, 1'),
    0x0802D329: dict(name='ShieldHit', sem='shield soldier: turn around / shield graphics (cosmetic), 1'),
    0x0803C8E1: dict(name='SetHitDir', sem='obj+0x290 = hit direction (knockback dir), 1'),
    0x08034109: dict(name='IfHitFromFront', children=[0xC], sem='(hit dir ^ 1) == facing -> child'),
    0x0803A79D: dict(name='HealFraction', params=[(0xC, 'frac', 'frac16')], sem='heal final damage * frac >> 16 (mult 0 -> 1)'),
}
# onHit function of registry nodes (word +4 of the def), with the behaviour we implement
ONHIT_NAMES = {
    0x08019299: 'SubState: onHit of state <state> aux',
    0x08018921: 'GotoState: replace top, 1',
    0x0801900B: 'Sequence',
    0x08018FDB: 'Selector',
    0x0800A9B5: 'Idle: 0',
    0x080188CB: 'Chance: chance(p) -> child',
    0x08039877: 'IfTimeInState: now - obj+0x210 > frames -> child',
    0x0801D0A1: 'Succeed: 2',
    0x0801888D: 'PushState: push, 1',
    0x08029E9F: 'IfCounterEq: counter == n -> child',
    0x0802FFDD: 'AbsorbPose: world msg 0x1D, 1',
    0x0802AA0D: 'MarkTime: obj+0x1E8 = now, 0',
    0x08029EC5: 'IncCounter: counter++, 0',
    0x0802AA5F: 'IfCooldownOver: now - obj+0x1E8 > frames -> child',
    0x080296A3: 'SetCounter: counter = n, 0',
    0x0801883B: 'Block: guard anim 7, blocking = 1, 1',
    0x080299CB: 'StunHold: hold controller, 1',
    0x0803123B: 'HoverDrift: impulse along the hit dir, 1',
    0x0803C08D: 'GlideToWaypoint: 1 if the player swings (anim slot 0x13) else 2',
    0x08018E63: 'PopState: pop, 2',
}


def is_aux_def(d):
    return 0x08000000 <= d < 0x08800000 and (is_def(d) or (u32(d) == 0 and u32(d + 4) in AUX_ONLY))


def node_info(d):
    fn = u32(d)
    if fn == 0:
        return fn, AUX_ONLY[u32(d + 4)], True
    r = REGISTRY.get(fn) or EXTRA_CREATE.get(fn)
    return fn, r, False


def decode(d, depth=0):
    if not is_aux_def(d) or depth > 40:
        return dict(addr=hex(d), invalid=True)
    fn, r, aux_only = node_info(d)
    on_hit = u32(d + 4)
    node = dict(addr=hex(d), fn=hex(fn), onHit=hex(on_hit))
    if r is None:
        node['name'] = f'node_{fn:08x}'
        node['raw_words'] = [hex(u32(d + 4 * k)) for k in range(6)]
        return node
    node['name'] = r['name']
    ps = {}
    for off, name, unit in r.get('params', []):
        v = u32(d + off)
        ps[name] = dict(raw=v, unit=unit, **convert(unit, v))
    if ps:
        node['params'] = ps
    if aux_only or fn in EXTRA_CREATE:
        offs = r.get('children', [])
    else:
        offs = child_offsets(fn, d)
    kids = [u32(d + o) for o in offs]
    kids = [k for k in kids if is_aux_def(k)]
    if kids:
        node['children'] = [decode(k, depth + 1) for k in kids]
    return node


def main():
    ai = json.load(open(os.path.join(ROOT, 'data/buu/enemy_ai.json')))
    states = sorted({int(k, 16) for k in ai['states']} | set(range(0x12)))
    # also every state referenced by aux trees (PushState / GotoState / SubState targets)
    out = dict(source='tools/battle/gen_buu_ai_extra.py (ROM decode)', onhit_protocol=__doc__.split('onHit return protocol')[1].split('usage:')[0].strip(),
               onhit_fns={hex(k): v for k, v in ONHIT_NAMES.items()},
               aux_only={hex(k): v for k, v in AUX_ONLY.items()},
               extra_nodes={hex(k): v for k, v in EXTRA_CREATE.items()},
               aux={}, reactions={}, death={})
    todo = list(states)
    seen = set()
    unknown = {}

    def scan(n):
        if n.get('name', '').startswith('node_'):
            unknown[n['fn']] = n
        p = n.get('params', {}).get('state')
        if p and n['name'] in ('PushState', 'GotoState', 'SubState'):
            todo.append(p['raw'])
        for c in n.get('children', []):
            scan(c)

    while todo:
        s = todo.pop()
        if s in seen or s > 0x400:
            continue
        seen.add(s)
        aux = u32(STATE_TABLE + 8 * s + 4)
        t = decode(aux)
        out['aux'][hex(s)] = t
        scan(t)
        if s < 0x12 or hex(s) not in ai['states']:
            r = decode(u32(STATE_TABLE + 8 * s))
            out['reactions'][hex(s)] = r
            scan(r)
    for t in range(N_ENEMIES):
        dd = u32(ENEMY_TABLE + ENEMY_SIZE * t + 0x14)
        w = [u32(dd + 4 * k) for k in range(4)]
        fn = w[0]
        name = (REGISTRY.get(fn) or EXTRA_CREATE.get(fn) or {}).get('name', f'node_{fn:08x}' if fn else 'special')
        out['death'][hex(t)] = dict(addr=hex(dd), create=hex(fn), name=name, f2=hex(w[2]), param=w[3])
    path = os.path.join(ROOT, 'data/battle/buu_ai_extra.json')
    json.dump(out, open(path, 'w'), indent=None, separators=(',', ':'))
    print('wrote', path, len(out['aux']), 'aux trees', len(out['reactions']), 'extra behaviour trees', 'unknown nodes:', sorted(unknown))


main()
