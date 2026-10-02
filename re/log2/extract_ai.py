"""Extract LoG2 enemy AI (state table 0x086d5194, 152 states) -> data/log2/ai.json.

Engine (VERIFIED by disassembly of 0x0801a9cc..0x0801ab64, 0x0800e5a4):
  * Every enemy owns an AI controller at entity+0x19c: +0 owner, +4 stack depth (max 5), +8.. stack of
    (runtime node object*, state-table entry*) pairs, +0x30 pending-destroy count, +0x34.. pending list.
  * State table 0x086d5194: 152 x [behaviour_root_node*, on_hit_root_node*].
    push(state)=0x0801aa98, replace-top(state)=0x0801aa46, pop=0x0801aad4, tick=0x0801ab08 (called from enemy
    update 0x0800e5a4 every frame), on-hit dispatch=0x0801ab2a (called from enemy onMessage 0x0800e5b8 after damage
    is computed: calls top-state on_hit_root.fn1(node, entity, hit)).
  * The initial state is the class row's ai_state (class table +8): pushed by enemy init 0x0800e280.
  * Nodes are ROM structs: word0 = create(node, entity) -> runtime object (vtable: [enter, update->done?, exit,
    destroy]), word1 = onHit(node, entity, hit) -> handled?, word2 = reset/exit helper, then type-specific params.
    Nodes whose word0 is 0 are on-hit-only nodes (identified by word1).
Distances are measured centre-to-centre in pixels on the collision boxes (box from 0x0800793a: x>>8,y>>8,w=+0x154,h=+0x155);
radius params are stored squared. Speeds are in 1/256 px per frame. Probabilities are per-frame, out of 65536 (rand16 < p).
Facing byte entity+0x10: 0=down,1=up,2=left,3=right."""
from en_lib import *
import json, os
OUTDIR=ROOT+'data/log2/'
S32=lambda v: v-(1<<32) if v&0x80000000 else v
# fn -> (name, params layout, semantics, evidence)
CAT={
 0x0801b63f:('Sequence',['n'],'Runs children in order; enter=child0.enter; when a child update returns done, exit it and enter the next; done after last. onHit: first child whose onHit returns true (0x0801b689). Runtime vtable 0x080247ec (0x0801b5ab/0x0801b5c1/0x0801b623).','VERIFIED(disasm)'),
 0x0801b887:('RaceFirst',['n'],'Enters all children; each frame updates children in order and returns done as soon as one returns done (short-circuit, later children not ticked that frame). vtable 0x08024548 (update 0x0801b80b).','VERIFIED(disasm)'),
 0x0801b861:('RaceAll',['n'],'Like RaceFirst but ticks every child each frame (OR of results). vtable 0x08024d98 (update 0x0801b7db).','VERIFIED(disasm)'),
 0x0801aef3:('IfPlayerWithin',['r2','c','c'],'Each frame: if player exists and dist^2(player centre, self centre) <= r2 run child A (near) else child B (far); switching exits the old child and enters the new one. Result = running child result. onHit routed the same way (0x0801af2b). vtable 0x0802413c.','VERIFIED(disasm+emu)'),
 0x0801d749:('IfPlayerSeen',['r2','c','c'],'Child A (seen) when player within r and NOT directly behind the enemy (behind = facing points away from player along the dominant axis, test 0x0801d5d0); else child B. vtable 0x08025014.','VERIFIED(disasm+emu: wolf r=80, snake r=72)'),
 0x0801ba47:('Chance',['prob','c'],'Each frame with probability p/65536 (rand16<p, 0x0802142e) ticks the child and returns its result, otherwise returns not-done. onHit likewise gated (0x0801ba69). vtable 0x08023d58.','VERIFIED(disasm)'),
 0x0801c05b:('WhileInMeleeRange',['c'],'Ticks child only while the player hurtbox overlaps the enemy melee box (own hurtbox +3px right, -4px left, -6px bottom; 0x0801beb8). vtable 0x0802410c.','VERIFIED(disasm)'),
 0x0801ea59:('WhileInMeleeRangeB',['c'],'Same with a slightly different box (0x0801e8b4: +2 left/top, -3 right, -6 bottom).','VERIFIED(disasm)'),
 0x0801ab65:('GotoState',['st'],'When updated (or on hit, via onHit 0x0801ab7b) replaces the top of the AI stack with state st (0x0801aa46).','VERIFIED(disasm+emu)'),
 0x0801a71d:('PushState',['st'],'Pushes state st on the AI stack (0x0801aa98) when updated / on hit (0x0801a733).','VERIFIED(disasm)'),
 0x0801b511:('SubState',['st'],'Inlines state st: create = st.behaviour.create, onHit = st.on_hit.onHit.','VERIFIED(disasm)'),
 0x0801bbad:('Wait',['f'],'Counts down f frames then done (runtime 0x08023bf0, update 0x0801bb95).','VERIFIED(disasm)'),
 0x0801c95f:('WaitPose',['f'],'Wait f frames while showing a pose/flash anim object (0x0802455c).','VERIFIED(disasm)'),
 0x0801b333:('ChasePlayer',['spd'],'Moves straight toward the player: velocity = (dx,dy)*spd/max(|dx|,|dy|) (so spd px/frame along the dominant axis), wall-sliding (tries full, x-only, y-only via 0x0800e45e); faces dominant axis; never completes.','VERIFIED(disasm+emu: snake 179 -> 0.699px/f, wolf 256 -> 1.0px/f)'),
 0x0801b4f3:('FleePlayer',['spd'],'Same as ChasePlayer but away from the player; done when blocked by walls.','VERIFIED(disasm)'),
 0x0801b15f:('AlignWithPlayer',['spd'],'Moves along the minor axis until within 3px of the player row/column, then faces the player along the major axis and completes (used before firing straight projectiles).','VERIFIED(disasm)'),
 0x0801c2ad:('BackAway',['spd'],'Faces the player while moving away at spd; never completes.','VERIFIED(disasm)'),
 0x0801c431:('BackAwayUntilBlocked',['spd'],'Faces player, moves away; completes when movement is blocked.','VERIFIED(disasm)'),
 0x0801a947:('Knockback',['p'],'Hit reaction: direction = hit direction (entity+0x18d from hit byte 0); speed k=(rand(64)+96)*p>>7; velocity along hit dir = 8*k /256 px/f, perpendicular jitter (rand(32768)-16384)*k>>12; plays hurt anim (state 2) and slides until the anim ends. Enemy stays hittable.','VERIFIED(disasm)'),
 0x0801acb7:('KnockbackInvulnerable',['p'],'As Knockback but clears entity+0x168 (hittable flag) for the duration and sets it back on exit (0x0801acad): invincibility frames = hurt animation length.','VERIFIED(disasm)'),
 0x0801be8b:('MeleeStrike',['p'],'On enter: if the player is alive, builds a melee hitbox from the enemy box extended 3-4px toward the player, faces the player, broadcasts a hit (attack table id 2 = melee power 1024, scale 1024; damage uses enemy STR) to the player, plays random hit sfx (rand(3)+0x22), then plays attack anim 6+rand(p). Update waits for the anim to end.','VERIFIED(disasm)'),
 0x0801b991:('ContactDamage',[],'Touch damage: every frame, if >48 frames since last hit and the player overlaps the enemy attack box, hit the player with attack id 2 (melee 1024).','VERIFIED(disasm)'),
 0x0801c7f3:('Charge',['spd','p'],'Dash at spd/256 px/f along the initial direction to the player; on contact hits with attack id 2; ends after passing the player by more than p px (or blocked).','VERIFIED(disasm)'),
 0x0801d255:('Pounce',['spd','p'],'Leap toward player at spd (sfx 0x81), contact hit with attack id 2, ends after max distance p.','VERIFIED(disasm), details INFERRED'),
 0x0801e145:('ProjectileVolley',['p'],'Boss/flyer special: spawns projectiles (0x0801e2a0/0x0801e224, gfx data 0x08703294); attack id 3 (energy 1024).','INFERRED'),
 0x0801a7df:('DeathExplode',['raw','p'],'Death node (class +0x14): fn2 0x0801a74d awards exp (0x0800e4d2), queues on-death event (0x0800e39c -> spawn on-death script), explosion effect (0x0800fa1c, size 0x28) and random death sfx (rand(2)+0x1f).','VERIFIED(disasm)'),
 0x0801a825:('Nop',[],'Does nothing forever (static runtime 0x0300220c). As a death node (0x086ff3c4) its fn2 0x0801a82d awards exp and queues the on-death event without explosion.','VERIFIED(disasm)'),
 0x0801a98d:('Patrol',[],'Follows the spawn waypoints (entity+0x190, 0x0800d156): cycles through the waypoint list; walk-to waypoints move at param/256 px/f.','VERIFIED(disasm+emu: wolf waypoint param 140 -> 0.547px/f)'),
 0x0801a9a5:('PopState',[],'Pops the AI stack when updated / on hit (0x0801aad4).','VERIFIED(disasm)'),
 0x0801c451:('ClassDefaultState',[],'Delegates to the class row ai_state: create = states[class.ai_state].behaviour.create; onHit = states[class.ai_state].on_hit.onHit (0x0801c46b).','VERIFIED(disasm)'),
 0x0801bb6d:('ShootEnergyBall',[],'Faces player; plays anim object 0x03002218 (vtable 0x080251b4): 32 frames, at frame 8 (if pool room) spawns projectile 0x080160c0 (vtable 0x08024258; attack id 1 energy 1024). Done when the anim ends.','VERIFIED(disasm), attack id INFERRED'),
 0x0801bc6f:('ShootKiBlast',[],'Faces player; anim object 0x03002224 (vtable 0x08025a70): 32 frames, at frame 16 spawns projectile via 0x0800ead8/0x0800e8b0 (attack id 3 energy 1024). Done when anim ends.','VERIFIED(disasm), attack id INFERRED'),
 0x0801bb89:('Succeed',[],'Static node: update returns done immediately; onHit returns handled=true (stops on-hit processing).','VERIFIED(disasm)'),
 0x0801bca9:('HitFlash',[],'Attaches hit-flash anim object (vtable 0x08024a3c) to the entity (also usable as on-hit node).','VERIFIED(disasm)'),
 0x0801bcf1:('StunAnim',[],'Sets the stunned/dizzy anim controller 0x03002230.','VERIFIED(disasm)'),
 0x0801bd17:('ClearAnim',[],'Removes the overlay anim (0x080076bc).','VERIFIED(disasm)'),
 0x0801c085:('SetIdleAnim',[],'Sets body anim state 0 (idle) via vtable slot 0x28.','VERIFIED(disasm)'),
 0x0801c0d7:('FacePlayer',[],'Turns to face the player (0x0800df98) and completes; on-hit: turn to face the attacker.','VERIFIED(disasm)'),
 0x0801c5a3:('GlowFlash',[],'On-hit/visual: attaches a 12-frame palette-cycle anim object (vtable 0x08024a14, table 0x086fdfbc).','VERIFIED(disasm)'),
 0x0801ca43:('FireKiBlast',[],'Spawns a straight projectile (0x080135e0, gfx 0x086e1590) with attack id 3 (energy 1024) toward facing; sfx.','VERIFIED(disasm)'),
 0x0801ca85:('PlaySound',['p'],'Plays sound effect id p.','VERIFIED(disasm)'),
 0x0801cca9:('LobProjectile',[],'Faces player and lobs an arcing projectile (0x0801ccc8: gravity 0x20/frame) that hits with attack id 1 (energy 1024) on landing.','VERIFIED(disasm)'),
 0x0801cf73:('ThrowProjectile',[],'Faces player; anim object 0x03002270: at frame 8 spawns projectile 0x0801a0b0 (attack id 1 INFERRED). Used by Ninjas.','INFERRED'),
 0x0801c82d:('FlyerAnimA',[],'Flyer (hawk/pterodactyl/ladybug) anim helper (static runtime 0x03002258).','INFERRED'),
 0x0801c85b:('FlyerAnimB',[],'Flyer anim helper (static runtime 0x0300225c, enter 0x0801c835 sets walk/fly anim).','INFERRED'),
 0x0801dbb3:('BossSpecialMelee',['raw'],'Boss special melee (data 0x08701e1c, attack id 2). Used by Cell forms / fighters (states 89,127).','INFERRED'),
 0x0801dcdb:('BossSpecialDirect',['raw'],'Boss special that calls the damage function 0x0800f910 directly (data 0x08701e4c; attack id 3). Used by Destroyer robots (state 94).','INFERRED'),
 0x0801e5c1:('BossSpecialProjectile',['raw'],'Boss special projectile (data 0x08703914; attack id 3). Used by General Tao (state 131).','INFERRED'),
 0x0801e77b:('BossBeam',['raw'],'Boss special projectile/beam (gfx 0x086e3458/0x086e35d8). Used by Cell Nonperfect/Perfect (states 120,142).','INFERRED'),
}
MSG={
 0x0801a6e1:('ApplyDamage',[],'On hit: if hit.scale!=0, store hit direction in entity+0x18d and call entity vtable+0x38 (enemy takeHit 0x0800e485: HP -= hit.damage; death node when HP<=0).','VERIFIED(disasm+emu)'),
 0x0801a805:('IfAttackId',['aid','c'],'On hit: forward to child if attack.id_byte==param OR attack.type_byte==param (attack desc = hit+8).','VERIFIED(disasm)'),
 0x0801acd5:('IfKnockbackBlocked',['c','c'],'On hit: probes the collision map at the knockback destination (offset table 0x086ff40c by hit direction, 0x08006366). Destination blocked by a wall -> child A (in generic handlers: KnockbackInvulnerable = i-frames, prevents wall juggling); destination free -> child B (plain Knockback, enemy stays hittable).','VERIFIED(disasm+emu: open ground -> state 2, hittable flag stays 1, 16px slide over 32 frames)'),
 0x0801c489:('HealFractionOfDamage',['frac'],'On hit: entity vtable+0x3c heal(param * hit.raw_damage >> 16) (enemy heal 0x0800e561 clamps at max HP).','VERIFIED(disasm)'),
}
def node_json(addr, depth=0, path=()):
    f=u32(addr)
    tbl=CAT
    if f==0 and u32(addr+4) in MSG: f=u32(addr+4); tbl=MSG
    if f not in tbl:
        return dict(addr=hex(addr),type='unknown',fn0=hex(u32(addr)),fn1=hex(u32(addr+4)))
    nm,lay,_,_=tbl[f]
    d=dict(addr=hex(addr),type=nm)
    k=3; kids=[]
    for kind in lay:
        w=u32(addr+4*k)
        if kind=='n':
            kids=[u32(addr+4*(k+1+j)) for j in range(w)]; k+=1+w
        elif kind=='c': kids.append(w); k+=1
        elif kind=='r2': d['radius_px']=round(w**0.5,2); d['radius_sq']=w; k+=1
        elif kind=='spd': d['speed_256ths']=w; d['speed_px_per_frame']=round(w/256,4); k+=1
        elif kind=='prob': d['p_65536']=w; d['percent_per_frame']=round(w/655.36,2); k+=1
        elif kind=='frac': d['heal_frac_65536']=w; k+=1
        elif kind=='f': d['frames']=w; k+=1
        elif kind=='st': d['state']=w; k+=1
        elif kind=='aid': d['attack_id_or_type']=w&0xff; k+=1
        elif kind=='p': d['param']=S32(w); k+=1
        elif kind=='raw': d['data']=hex(w); k+=1
    if kids:
        if addr in path or depth>14: d['children']='(cycle)'
        else: d['children']=[node_json(c,depth+1,path+(addr,)) for c in kids]
    return d
def walk(n, fn):
    fn(n)
    for c in n.get('children',[]) if isinstance(n.get('children'),list) else []: walk(c,fn)
def render(n, ind=0, out=None):
    out=out if out is not None else []
    extra=[f'{k}={v}' for k,v in n.items() if k not in ('addr','type','children')]
    out.append('  '*ind+f"{n['type']}({', '.join(extra)}) @{n['addr']}")
    if isinstance(n.get('children'),list):
        for c in n['children']: render(c,ind+1,out)
    return out
def states():
    res=[]
    for i in range(N_STATE):
        p0,p1=struct.unpack_from('<2I',ROM,(STATE_TBL&0x1ffffff)+8*i)
        b=node_json(p0); h=node_json(p1)
        res.append(dict(id=i,addr=hex(STATE_TBL+8*i),behaviour=b,on_hit=h,
                        text=render(b)+['-- on hit:']+render(h,1)))
    return res
def summarize(st_list, start):
    """Follow state transitions from `start`; collect radii, speeds, attacks, reactions."""
    seen=set(); todo=[start]; info=dict(states=[],radii_px=set(),speeds={},attacks=set(),waits=set(),
        stun_on_attack_ids=set(),energy_absorb=False,uses_patrol=False,aggro_radius_px=None,aggro_requires_facing=None)
    ATT={'MeleeStrike':'melee strike (attack id 2, melee, power 1024; STR-based)','ContactDamage':'touch damage (attack id 2; 48f cooldown)',
         'Charge':'charge/dash (attack id 2 on contact)','Pounce':'pounce (attack id 2 on contact)','ShootEnergyBall':'energy ball projectile (attack id 1, energy 1024; POW-based)',
         'ShootKiBlast':'ki blast projectile (attack id 3, energy 1024; POW-based)','FireKiBlast':'ki blast projectile (attack id 3)','LobProjectile':'lobbed arcing projectile (attack id 1)',
         'ThrowProjectile':'thrown projectile (attack id 1, INFERRED)','ProjectileVolley':'projectile volley (attack id 3, INFERRED)','BossSpecialMelee':'boss special melee (INFERRED)',
         'BossSpecialDirect':'boss special (INFERRED)','BossSpecialProjectile':'boss special projectile (INFERRED)','BossBeam':'boss beam/projectile (INFERRED)'}
    first=st_list[start]['behaviour']
    if first['type'] in ('IfPlayerSeen','IfPlayerWithin'):
        info['aggro_radius_px']=first.get('radius_px'); info['aggro_requires_facing']=(first['type']=='IfPlayerSeen')
    while todo:
        s=todo.pop()
        if s in seen or s>=len(st_list): continue
        seen.add(s); info['states'].append(s)
        def visit(n, onhit=False):
            t=n['type']
            if t in ('GotoState','PushState','SubState'): todo.append(n['state'])
            if t in ('IfPlayerWithin','IfPlayerSeen'): info['radii_px'].add(n['radius_px'])
            if 'speed_px_per_frame' in n: info['speeds'].setdefault(t,set()).add(n['speed_px_per_frame'])
            if t in ATT: info['attacks'].add(ATT[t])
            if t=='Wait': info['waits'].add(n['frames'])
            if t=='Patrol': info['uses_patrol']=True
            if t=='HealFractionOfDamage': info['energy_absorb']=True
            if t=='IfAttackId' and isinstance(n.get('children'),list): info['stun_on_attack_ids'].add(n['attack_id_or_type'])
        walk(st_list[s]['behaviour'],visit); walk(st_list[s]['on_hit'],lambda n: visit(n,True))
    out={k:(sorted(v) if isinstance(v,set) else v) for k,v in info.items()}
    out['speeds']={k:sorted(v) for k,v in info['speeds'].items()}
    out['states']=sorted(info['states'])
    return out
if __name__=='__main__':
    os.makedirs(OUTDIR,exist_ok=True)
    st=states()
    cat=[dict(fn=hex(k),name=v[0],params=v[1],semantics=v[2],evidence=v[3]) for k,v in CAT.items()]
    cat+=[dict(fn1=hex(k),name=v[0],params=v[1],semantics=v[2],evidence=v[3],on_hit_only=True) for k,v in MSG.items()]
    doc=dict(source='roms/log2.gba',state_table=hex(STATE_TBL),n_states=N_STATE,engine=__doc__,node_types=cat,states=st)
    json.dump(doc,open(OUTDIR+'ai.json','w'),indent=1)
    unk=sum(1 for s in st for x in [0] if 'unknown' in json.dumps(s))
    print('states',len(st),'states containing unknown nodes',unk)
