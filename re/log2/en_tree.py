"""Decode LoG2 enemy AI node trees (state table 0x086d5194)."""
from en_lib import *
S=lambda v: v-(1<<32) if v&0x80000000 else v
# type fn0 -> (name, layout) ; layout: list of field kinds after the 3 fn words
#  'p' int param, 'r2' squared radius (px^2), 'spd' speed (1/256 px per frame), 'n' count+children, 'c' child, 'st' state id,
#  'prob' 16-bit probability, 'f' frames, 'raw' opaque
TYPES={
 0x801b63f:('Sequence',['n']),
 0x801b887:('RaceFirst',['n']),
 0x801b861:('RaceAll',['n']),
 0x801aef3:('IfPlayerWithin',['r2','c','c']),
 0x801d749:('IfPlayerSeen',['r2','c','c']),
 0x801ba47:('Chance',['prob','c']),
 0x801c05b:('WhileInMeleeRange',['c']),
 0x801ea59:('WhileInMeleeRangeB',['c']),
 0x801ab65:('GotoState',['st']),
 0x801a71d:('PushState',['st']),
 0x801b511:('SubState',['st']),
 0x801bbad:('Wait',['f']),
 0x801c95f:('WaitPose',['f']),
 0x801b333:('ChasePlayer',['spd']),
 0x801b4f3:('FleePlayer',['spd']),
 0x801b15f:('AlignWithPlayer',['spd']),
 0x801c2ad:('BackAway',['spd']),
 0x801c431:('BackAwayUntilBlocked',['spd']),
 0x801a947:('Knockback',['p']),
 0x801acb7:('KnockbackInvulnerable',['p']),
 0x801be8b:('MeleeStrike',['p']),
 0x801c7f3:('Charge',['spd','p']),
 0x801d255:('Pounce',['spd','p']),
 0x801e145:('ProjectileVolley',['p']),
 0x801be8b+0:('MeleeStrike',['p']),
 0x801a7df:('DeathExplode',['raw','p']),
 0x801a825:('Nop/DieQuietly',[]),
 0x801a98d:('Patrol',[]),
 0x801a9a5:('PopState',[]),
 0x801c451:('ClassDefaultState',[]),
 0x801b991:('ContactDamage',[]),
}
SINGLETON_DESC={
 0x801bb6d:'FacePlayer+ShootEnergyBall (anim 0x03002218/vt 0x080251b4: 32f, at f8 spawns projectile 0x080160c0 (attack id1))',
 0x801bb89:'Succeed (static, update returns done)',
 0x801bc6f:'FacePlayer+ShootKiBlast (anim 0x03002224/vt 0x08025a70: 32f, at f16 spawns projectile via 0x0800e8b0 (attack id3))',
 0x801bca9:'OnHit:FlashSprite (attach hit-flash anim object 0x08024a3c)',
 0x801bcf1:'OnHit:SetReactAnim (anim ctl 0x03002230)',
 0x801bd17:'OnHit:ClearAnim (0x080076bc)',
 0x801c085:'OnHit:SetIdleAnim (vt slot 0x28 state 0)',
 0x801c0d7:'FacePlayer (instant; also on-hit: turn to face player)',
 0x801c5a3:'OnHit:GlowFlash (attach 12-frame palette-flash anim obj vt 0x08024a14)',
 0x801ca43:'FireKiBlast (energy id3, projectile gfx 0x086e1590)',
 0x801ca85:'PlaySound(param)',
 0x801cca9:'LobProjectile (arcing, energy id1)',
 0x801c82d:'Hop/teleport anim (0x086ff59c)',
 0x801c85b:'Hop target (0x086ff5a8)',
 0x801cf73:'Boss special (0x086ffc24)',
 0x801dbb3:'Boss special melee (0x08701e1c, id2)',
 0x801dcdb:'Boss special (0x08701e4c, direct damage calc)',
 0x801e5c1:'Boss special projectile (0x08703914, id3)',
 0x801e77b:'Boss special projectile (gfx 0x086e3458/0x086e35d8)',
}
def name_of(f):
    if f in TYPES: return TYPES[f][0]
    return SINGLETON_DESC.get(f,'Unknown_%x'%f)
MSG_TYPES={
 0x801a6e1:('OnHit:ApplyDamage',[]),
 0x801c489:('OnHit:HealFractionOfDamage',['frac']),
 0x801a805:('OnHit:IfAttackId',['aid','c']),
 0x801acd5:('OnHit:IfKnockbackPathClear',['c','c']),
}
def node(addr, depth=0, seen=None, out=None, maxd=12):
    if out is None: out=[]
    seen=seen if seen is not None else set()
    f=u32(addr); ind='  '*depth
    if f==0 and u32(addr+4) in MSG_TYPES: f=u32(addr+4)
    TY=dict(TYPES); TY.update(MSG_TYPES)
    if f not in TY:
        out.append(f'{ind}{name_of(f)} @{addr:#x}')
        return out
    nm,lay=TY[f]
    vals=[]; kids=[]; k=3
    for kind in lay:
        w=u32(addr+4*k)
        if kind=='n':
            cnt=w; kids=[u32(addr+4*(k+1+j)) for j in range(cnt)]; vals.append(f'n={cnt}'); k+=1+cnt
        elif kind=='c': kids.append(w); k+=1
        elif kind=='r2': vals.append(f'r={int(w**0.5)}px(r2={w})'); k+=1
        elif kind=='spd': vals.append(f'speed={w}/256px/f({w/256:.2f})'); k+=1
        elif kind=='prob': vals.append(f'p={w}/65536({w/655.36:.1f}%/frame)'); k+=1
        elif kind=='f': vals.append(f'{w}f'); k+=1
        elif kind=='st': vals.append(f'state={w}'); k+=1
        elif kind=='frac': vals.append(f'heal={w}/65536 of raw damage ({w/655.36:.0f}%)'); k+=1
        elif kind=='aid': vals.append(f'attack_id_or_type={w&0xff}'); k+=1
        else: vals.append(f'{S(w)}'); k+=1
    out.append(f'{ind}{nm}({", ".join(vals)}) @{addr:#x}')
    if addr in seen or depth>maxd:
        if kids: out.append(f'{ind}  ...');
        return out
    seen=seen|{addr}
    for c in kids: node(c, depth+1, seen, out, maxd)
    return out
def state(i):
    p0,p1=struct.unpack_from('<2I',ROM,(STATE_TBL&0x1ffffff)+8*i)
    return p0,p1
if __name__=='__main__':
    import sys
    ids=[int(x) for x in sys.argv[1:]] or range(N_STATE)
    for i in ids:
        p0,p1=state(i)
        print(f'== state {i}: behaviour {p0:#x}, on-hit {p1:#x}')
        print('\n'.join(node(p0,1)))
        print('  -- on-hit:')
        print('\n'.join(node(p1,2)))
