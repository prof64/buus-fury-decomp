"""Enemy attack + projectile catalog for LoG2 (used by extract_enemies.py / extract_bosses.py).
Numbers are read from ROM tables where a table exists; timings from disassembly; 'measured' fields come from the
arena harness (bs_watch.py / bs_special.py / bs_measure.py, see notes Part C)."""
from en_lib import *
S=lambda v: v-(1<<32) if v>>31 else v
def vel4(a): return {d:[S(u32(a+8*i))/256,S(u32(a+8*i+4))/256] for i,d in enumerate(('down','up','left','right'))}
def frame4(a,stride=12):
    out={}
    for i,d in enumerate(('down','up','left','right')):
        b=ROM[(a+stride*i)&0x1ffffff:((a+stride*i)&0x1ffffff)+4]
        sb=lambda x: x-256 if x>127 else x
        out[d]=dict(origin_offset=[sb(b[0]),sb(b[1])],w=b[2],h=b[3])
    return out
def offs4(a): return {d:[S(u32(a+8*i)),S(u32(a+8*i+4))] for i,d in enumerate(('down','up','left','right'))}
ATK_NOTE='attack table 0x086db2cc: id1 energy 1024, id2 melee 1024, id3 energy 1024, id10 energy 64 (player beam id)'
TEAM_NOTE='hit packet team byte: 0 = hits only the player side, 1 = player attacks, 2 = hits everyone'
def projectiles():
    P=[]
    P.append(dict(key='ki_blast',ctor='0x0800e8b0',vtable='0x08024cbc',size=0x54,
        spawned_by='ShootKiBlast node 0x0801bc6f: anim object 0x03002224 (vtable 0x08025a70), 32-frame anim, spawn at anim frame 16 via 0x0800ead8 (pool check)',
        speed_px_per_frame=3.0,velocity_table='0x0840e554',velocity_by_facing=vel4(0x0840e554),
        hitbox=dict(w=16,h=16,anchor='centred on projectile position',frame_table='0x0840e47c'),
        attack_id=3,power=1024,scale=1024,team='owner+0x16c (0 for enemies)',
        aim='straight along shooter facing (shooter first faces the player and usually aligns via AlignWithPlayer); no homing',
        lifetime='until it leaves the 240x160 camera view (0x08013c22), hits a wall (0x08005d6c) or hits a target; then 16-frame explosion (frame 0x0840e53c)',
        pierce=False,cancel_rules='destroyed when overlapped by a player energy attack (onMessage 0x0800ea3b: query type 0 with attack type byte 1)',
        sfx='fire: rand(2)+0x0d; hit: rand(3)+0x22',
        measured=dict(speed=3.0,spawn_after_state_entry_frames=16,typical_life_to_hit_at_100px='41-48 f',evidence='bs_watch/bs_measure (Android 17/18/19/20, Cooler, Cell Jr, Yamcha, Krillin, Tien, Tao, Cells)'),
        evidence='VERIFIED(disasm+emu)'))
    P.append(dict(key='energy_ball',ctor='0x080160c0',vtable='0x08024258',size=0x4c,
        spawned_by='ShootEnergyBall node 0x0801bb6d: anim object 0x03002218 (vtable 0x080251b4), 32-frame anim, spawn at anim frame 8 (pool >= 0x80)',
        speed_px_per_frame=1.75,velocity_table='0x086e5ba8',velocity_by_facing=vel4(0x086e5ba8),
        hitbox=dict(by_facing=frame4(0x086e5dc8),note='rect = position - origin_offset, size w x h (0x08013bf4)'),
        attack_id=1,power=1024,scale=1024,team=0,aim='straight along facing; no homing',
        lifetime='until outside the camera rect, wall hit (explodes, effect 0x08016124) or target hit (explodes)',
        pierce=False,cancel_rules='destroyed by a player energy attack overlap (type 0) or by player attack ids 8 / 14 (type 2 query)',
        measured=dict(speed=1.75,spawn_after_state_entry_frames=8,rect='16x16',evidence='bs_watch class 85 henchman'),
        evidence='VERIFIED(disasm+emu)'))
    P.append(dict(key='thrown_star',ctor='0x0801a0b0',vtable='0x08024f0c',size=0x4c,
        spawned_by='ThrowProjectile node 0x0801cf73: anim object 0x03002270 (vtable 0x080256d8), spawn at anim frame 8',
        speed_px_per_frame=1.75,velocity_table='0x086fe06c',velocity_by_facing=vel4(0x086fe06c),
        hitbox=dict(w=8,h=8,origin_offset=[4,4],frame_table='0x086fe048',note='2-frame spin anim, toggles every 8 frames'),
        attack_id=1,power=1024,scale=1024,team=0,aim='straight along facing',
        lifetime='until outside camera rect, wall (vanishes, no explosion) or target hit',pierce=False,
        cancel_rules='same as energy_ball',measured=dict(speed=1.75,rect='8x8',evidence='bs_special ninja_throw'),evidence='VERIFIED(disasm+emu)'))
    P.append(dict(key='lob_rock',ctor='0x0801ccc8',vtable='0x08024204',size=0x54,
        spawned_by='LobProjectile node 0x0801cca9 (anim 0x0300226c/0x08024bxx): 32-frame anim, spawn at anim frame 16 at self + offset table 0x086ff8a8[facing]',
        ballistics=('ground velocity = (player centre - self centre) along the facing axis only (other axis zeroed) * 0x71d >> 8 '
                    '(1/256 px per frame) -> lands near the player after ~35 frames; height z starts at 8 px with vz 2 px/f, gravity '
                    '0x20/256 px/f^2 (0x0801cdfc)'),
        hitbox=dict(w=16,h=16,note='rect of the rock at landing'),attack_id=1,power=1024,scale=1024,team=2,
        aim='targets the player position at throw time along the facing axis',
        lifetime='until landing (z<0): broadcasts one hit (team 2: also hurts enemies), explosion + 4 debris (0x0801cd32)',pierce=False,
        measured=dict(airtime_frames=35,ground_speed_px_per_frame='1.95-2.9 (accelerating as drawn; includes height)',evidence='bs_watch class 9 tiger bandit at 70 px'),
        evidence='VERIFIED(disasm+emu)'))
    P.append(dict(key='laser',ctor='0x0801e224',vtable='0x08024e5c',size=0x38,
        spawned_by='LaserBeam node 0x0801e145 (param = max length px)',
        growth_px_per_frame=8,max_length='node param (64 for ladybugs)',muzzle_offsets=offs4(0x08703294+0xc),
        hitbox='segment from muzzle to tip widened by 4 px each side',attack_id=3,power=1024,scale=1024,team=0,
        hits='at most one hit per laser (+0x12 flag)',lifetime='60-frame anim then retracts 8 px/f; object ~68 frames',pierce=True,
        measured=dict(hit_after_start_frames='2-5',lifetime=68,evidence='bs_special ladybug_laser'),evidence='VERIFIED(disasm+emu)'))
    P.append(dict(key='boss_beam',ctor='0x080143a8',vtable='0x08024acc',
        spawned_by='BeamAttack 0x0801e77b (Cell Nonperfect data 0x08704788 / Perfect 0x087047d4) and BeamAttackTao 0x0801e5c1 (data 0x08703914)',
        note='the player Kamehameha beam class reused (see moves.json Kamehameha): head grows 3 px/f, 16x16 box at the head broadcast every frame',
        attack_id=10,power=0x40,scale=0x3000,team=0,duration_frames=90,
        effective_hit_rate='once per player invulnerability window (measured every 32 frames)',
        measured=dict(cell_nonperfect_dmg_on_END53='24-36 per hit (formula with S=POW 41, power 64, scale 12288 predicts 24-36)',
                      tao_dmg_on_END53='48-60',interval_frames=32,evidence='bs_special cellnp_beam / tao_special'),
        evidence='VERIFIED(disasm+emu)'))
    P.append(dict(key='spray_particle',ctor='0x0801dcfc',vtable='0x080250a8',size=0x58,
        spawned_by='SprayAttack 0x0801dcdb (Destroyer): 1 particle per frame from anim frame 4 to 127 (pool >= 0x500 and < 124 live)',
        speed_px_per_frame=2.0,velocity_table='0x08703250',velocity_by_facing=vel4(0x08703250),jitter='+-(rand(192)-96)/256 px/f on both axes',
        mouth_offsets=offs4(0x08701e2c),hitbox=dict(w=16,h=16,anchor='centred'),lifetime_frames=20,
        attack_id=3,power=1024,scale=51,team='direct to player',hits='once per particle; applied directly: no hitstun, no player i-frames',
        measured=dict(damage_per_particle_on_END53=2,hits_per_frame='~0.5-1',evidence='bs_special destroyer_spray'),evidence='VERIFIED(disasm+emu)'))
    return P
ATTACKS=[
 dict(key='melee_strike',node='MeleeStrike 0x0801be8b',attack_id=2,type='melee',power=1024,scale=1024,stat='STR',team=0,
      hitbox=('instant check on the first frame of the strike state against the player hurt rect: horizontal (|dx|>|dy|): '
              'y from own hurt-rect centre to bottom+3, x from own centre to right edge+3 (player to the right) or left edge-4 to centre; '
              'vertical: x = own collision box, y from hurt-rect top to bottom-6 (down) or to collision-box top (up) (0x0801bd2c)'),
      startup_frames=0,active_frames=1,recovery='strike anim (anim state 6+rand(variants)) + the Wait that follows in the AI (e.g. 30 f for ai 8)',
      measured=dict(snake_class53='damage on the state-entry frame; strike state 47 f = 17 f anim + Wait 30'),evidence='VERIFIED(disasm+emu)'),
 dict(key='contact_damage',node='ContactDamage 0x0801b991',attack_id=2,type='melee',power=1024,scale=1024,stat='STR',
      rule='player overlaps the enemy attack box -> hit; then no further contact hit for 48 frames (frame counter 0x03000e20)',
      used_by='only ai 15 (class 94 Triceratops herd)',evidence='VERIFIED(disasm+emu)'),
 dict(key='charge',node='Charge 0x0801c7f3',attack_id=2,type='melee',rule='dash at speed px/f along the initial direction to the player; first contact hits (once); ends after passing the player by the overshoot distance',used_by='hawks (1.5 px/f, 56 px), pterodactyls (2.0, 64)',evidence='VERIFIED(disasm)'),
 dict(key='pounce',node='Pounce 0x0801d255',attack_id=2,type='melee',rule='leap toward the player at speed up to range px (sfx 0x81), single contact hit',used_by='T-Rex/Triceratops King (2.0 px/f, 72 px), triceratops/warthog (1.2, 56)',evidence='VERIFIED(disasm), timings INFERRED'),
 dict(key='ki_blast',projectile='ki_blast',attack_id=3,stat='POW',used_by='fighter AIs 21/29/80/98/116/129/138/148 (ShootKiBlast)'),
 dict(key='energy_ball',projectile='energy_ball',attack_id=1,stat='POW',used_by='ai 11 Warlord henchmen (ShootEnergyBall)'),
 dict(key='thrown_star',projectile='thrown_star',attack_id=1,stat='POW',used_by='ai 52 ninjas'),
 dict(key='lob_rock',projectile='lob_rock',attack_id=1,stat='POW',used_by='ai 45 tiger bandits'),
 dict(key='laser',projectile='laser',attack_id=3,stat='POW',used_by='ai 106 ladybug robots'),
 dict(key='beam',projectile='boss_beam',attack_id=10,stat='POW',used_by='Cell Nonperfect/Perfect (states 120/142), General Tao (131)'),
 dict(key='spray',projectile='spray_particle',attack_id=3,stat='POW',used_by='Destroyer robots (state 94)'),
 dict(key='drain_grab',node='DrainGrab 0x0801dbb3',attack_id=2,type='melee',stat='STR',
      rule='player held 240 frames (mash A: -16 extra frames per press); hit at frame 8 (scale 256) then every 16 f (scale 128); Cell heals by damage dealt',
      used_by='Cell imperfect (state 89), Cell Nonperfect (state 127)',evidence='VERIFIED(disasm+emu)'),
 dict(key='self_destruct',node='SelfDestructBurst 0x0801ca43',attack_id=3,team=2,rule='Eggbot death: flash, 40-frame pose, 32x32 burst (team 2) then explode',used_by='Eggbots (death node 0x086d5694)',evidence='VERIFIED(disasm)'),
]
AI_SPECIALS={89:'drain_grab',127:'drain_grab',94:'spray',120:'beam',142:'beam',131:'beam',109:'laser',44:'self_destruct'}
NODE2ATK={'MeleeStrike':'melee_strike','ContactDamage':'contact_damage','Charge':'charge','Pounce':'pounce','ShootKiBlast':'ki_blast',
          'ShootEnergyBall':'energy_ball','ThrowProjectile':'thrown_star','LobProjectile':'lob_rock','LaserBeam':'laser',
          'BeamAttack':'beam','BeamAttackTao':'beam','SprayAttack':'spray','DrainGrab':'drain_grab','SelfDestructBurst':'self_destruct'}
def attack_keys(states_json, state_ids):
    keys=set()
    def walk(n):
        k=NODE2ATK.get(n.get('type'))
        if k: keys.add(k)
        if isinstance(n.get('children'),list):
            for c in n['children']: walk(c)
    for s in state_ids:
        walk(states_json[s]['behaviour']); walk(states_json[s]['on_hit'])
    return sorted(keys)
