"""Boss / story-opponent facts, group 'others'.  JSON-serialisable.  Units: px, px/frame, frames (60/s),
chances in % per frame (p/65536 rounded).  Tags: VERIFIED (emulator), DISASM (read from code/data),
INFERRED.  Trees: data/buu/enemy_ai.json (state ids below).  Evidence scripts: bv_others_*.py,
results work/buu/bv_others_verify.json.

Spawn-record condition word (rec+4, evaluated by 0x08004784 when an area loads) -- DISASM, VERIFIED for
the SW-forest records: 0 = always; 1..0xFFFF = never placed by the area loader (the encounter is started
by a script); (1<<16)|f = only if story flag f (GameData+0x55 bitfield) is set; (2<<16)|f = only if f is
clear; (3<<16)|f = if f set: clear it and spawn (one-shot); >= 0x40000 = pointer to a bytecode condition.
Boss records also carry rec+0x14 -> {handler 0x0800421F (inline code) | 0x0800424F (queue script header),
script}; VERIFIED it is NOT run on death (kill test bv_others_death.py); INFERRED encounter/cutscene hook.
"""

GENERIC_FIGHTER = dict(
    states='0xcb..0xd2 (Human 0x1b2..0x1b8: identical but no block)',
    aggro='IfPlayerWithin2 100 px (state 0xcb, idle animation until then)',
    priority_list_each_frame=[
        'block check (SubState 0xd2): after 90 frames in the state, if the player is attacking: 20%/f -> 0xd1 '
        'Block (held while the player keeps attacking; leaves with 5%/f once he stops)',
        'player within 50 px: 12%/f and in melee range -> 0xce MeleeAttack (melee sub 2 x1.0) + Wait 18',
        'within 70 px: 4%/f -> 0xcd StepToward 0.5977 px/f (ends 6%/f)',
        'farther than 90 px: 8%/f -> 0xcf Chase 0.5977 px/f (ends 2%/f, or melee 12%/f when in range)',
        '0.8%/f -> 0xd0 back off 0.7969 px/f while within 80 px (ends 0.4%/f or beyond 80)',
        '12%/f -> 0xcf chase',
        'else stand still facing the player'],
    status='DISASM (trees); Majin-Fighter-style timings VERIFIED by the AI worker (melee state 41-47 f)')

PROJECTILES = {
    'ninja_star': dict(
        builder='0x0802C790', vtable='0x0804BAA0', size=0x4C, update='0x0802C7EA',
        speed_px_per_frame=1.75, velocity_table='0x0875497C (per facing 0 down,1 up,2 left,3 right: +-448/256)',
        hitbox=dict(rect_table='0x08754A1C', x_off=-4, y_off=-4, w=8, h=8,
                    note='two rect frames alternating every 8 frames (spin); rect = pos - (b0,b1), size (b2,b3)'),
        lifetime_frames=None,
        lifetime='until its rect leaves the room bounds, it hits a wall (map collision 0x08009324) or it hits',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=1024, stat='owner POW vs END2',
                    path='hit test 0x08011C70 against the player list'),
        spawn='controller frame 8; offset per facing from 0x087548E8+8',
        status='VERIFIED (speed 448/256 px/frame, energy sub 1 x1.0 hit for 28.5 HP on Goku L60); hitbox DISASM'),
    'mummy_wrap': dict(
        builder='0x0802D79C(variant 0 straight | 1 diagonal)', vtable='0x0804A830', size=0x60, update='0x0802D940',
        speed_px_per_frame=1.75,
        velocity_table='0x08756B74: per facing {straight vx,vy, diagonal vx,vy}: down (0,448)/(-316,316), '
                       'up (0,-448)/(316,-316), left (-448,0)/(-316,-316), right (448,0)/(316,316)',
        hitbox=dict(rect_table='0x087571F4 (12 B per frame, 3 anim frames per kind)',
                    straight_vertical='8x16', straight_horizontal='16x8', diagonal='16x16'),
        lifetime='until off the room or into a wall; anim cycle 24 frames',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=1024, stat='owner POW'),
        on_hit=('if fewer than 8 wraps are already attached and it overlaps the player: attach after 4 frames '
                '(+0x50=1, follows the player at a fixed offset); while attached every wrap lowers the player '
                'walk (player+0x238) and dash (+0x23C) speed by 48/256 px/frame (count of attached wraps x 48, '
                'recomputed each frame 0x0802D8E6); else it bursts (puff 0x08712FDC)'),
        escape='8 new d-pad presses within 60 frames (0x08040284(input, 0xF0, 8, 60)) frees the player from all wraps',
        status='VERIFIED: 8 per burst; walk 352 -> 304 -> 256 with 1 and 2 wraps; freed on the 8th d-pad press'),
    'pikkon_barrage': dict(
        builder='0x080246B4 (via 0x080249E0)', vtable='0x0804BFCC', size=0x58,
        damage=dict(attack_type='0x086B80A8', kind='energy', sub=4, mult=51, stat='owner POW'),
        rate='one object per frame for 48 frames',
        status='VERIFIED: one hit per frame (~0.75 HP each on Goku L60 from Pikkon POW 14) at 24 px; '
               'flight path/hitbox not decoded (INFERRED short-range spray)'),
}

HEAVY_STRIKE = ('HeavyStrike 0x0802F919: 60-frame wind-up (action 4), then one melee hit (sub 2, x1.0) on a '
                'player within ~60 px centre-to-centre (VERIFIED hit at <=60, miss at 65; hit at frame 58, '
                'state ends at 76)')

BRUTE_TREE = dict(
    states='0x136..0x13b',
    aggro='none: stands facing the player until within 88 px (IfPlayerWithin)',
    loop=[
        '0x137: MarkTime; StandStill; then each frame:',
        '  beam guard (SubState 0x13b): >90 f in state and the player\'s beam line covers it -> 0x13a: Block '
        'while in the beam line, back to 0x137 30 f after leaving it',
        '  in melee range: 4%/f, if >120 f since the last strike (IfCooldown2Over 120) -> 0x138 HeavyStrike '
        '+ MarkTime2',
        '  in melee range: otherwise wait',
        '  8%/f -> 0x139 chase 0.1484 px/f; reaching melee range -> 0x138 strike at once (no cooldown check)'],
    note='no ordinary punch; the only attack is HeavyStrike',
    status='DISASM; strike VERIFIED')

BOSSES = {
    0x00: dict(name='Airship Warlord', archetype='slow brute',
               phases=[dict(states=['0x12', '0x13', '0x14'], trigger='spawn',
                            pattern='idle until within 72 px; chase 0.1992 px/f; in melee range 10%/f -> '
                                    'MeleeAttack + Wait 36')],
               attacks=[dict(name='punch', node='MeleeAttack', damage='melee sub 2 x1.0', recovery_frames=36)],
               movement=dict(chase_px_per_frame=0.1992), spawn='Control Room (12,28); cond 0x20095 = while flag '
                                                              '0x95 clear', status='DISASM'),
    0x06: dict(name='Blister', archetype='stationary spawner',
               phases=[dict(states=['0x25'], trigger='spawn', pattern='SpawnEnzymes: one Enzyme (0x18) every '
                            '120 frames at its position or +(16,-8), 8 in total, then idle forever')],
               attacks=[], movement='none', specials=['summons Enzyme x8 (VERIFIED)'],
               spawn='Stomach (21,4/6/8/13), cond 0 (always)', status='VERIFIED'),
    0x07: dict(name='Blood Cell', archetype='drifting contact hazard',
               phases=[dict(states=['0x26', '0x27'], trigger='player within 300 px',
                            pattern='DriftContact: straight-line drift in a random direction at 0.25/0.5/0.75 '
                                    'px/f (rand(3)+1); contact melee x1.0; backs off 16 frames after a hit')],
               attacks=[dict(name='contact', damage='melee sub 2 x1.0 (attacker STR 55)')],
               movement=dict(drift_px_per_frame=[0.25, 0.5, 0.75]),
               spawn='Stomach / Intestines, cond 0', verified=['drift 0.5 px/f straight line',
                                                               'contact hit melee x1.0'], status='VERIFIED'),
    0x18: dict(name='Enzyme', archetype='latcher (spawned by Blister)',
               phases=[dict(states=['0x8d', '0x8e'], trigger='spawn', pattern='chase 0.5 px/f until latched'),
                       dict(states=['0x8f'], trigger='IfLatchOverlap (body rects overlap; enzyme facing down)',
                            pattern='LatchDrain: glued to the player, -1 HP every 64 frames'),
                       dict(states=['0x90'], trigger='IfButtonMashed: >8 presses (A/B/d-pad) with gaps <= 15 f',
                            pattern='retreat 1.0 px/f while within 56 px, then chase again (0x8e)')],
               attacks=[dict(name='drain', damage='1 HP / 64 frames (fixed, not the damage formula)')],
               movement=dict(chase=0.5, retreat=1.0), status='VERIFIED (latch, drain); mash escape DISASM'),
    0x3f: dict(name="Pilaf's Guardian", archetype='brute', tree='BRUTE_TREE', resist=dict(melee=64, energy=128),
               phases=[dict(states=['0x13c', '0x136'], trigger='spawn', pattern='face the player until within 88 px'),
                       dict(states=['0x137', '0x138', '0x139', '0x13a', '0x13b'], trigger='player within 88 px',
                            pattern='see BRUTE_TREE')],
               attacks=[dict(name='heavy strike', node='HeavyStrike', windup_frames=58, total_frames=76,
                             reach_px=60, cooldown_frames=120, chance='4%/f in melee range',
                             damage='melee sub 2 x1.0 (STR 38)')],
               movement=dict(chase=0.1484), specials=['blocks while standing in the player beam line'],
               spawn="Pilaf's Castle (10,31) cond 0x2005e (while flag 0x5e clear); rec extra script #365",
               verified=['HeavyStrike timing and reach'], status='VERIFIED/DISASM'),
    0x64: dict(name='Shadow Colossus', archetype='brute', tree='BRUTE_TREE (same states 0x136..0x13b as the Guardian)',
               resist=dict(melee=96, energy=64), attacks=[dict(name='heavy strike', node='HeavyStrike',
                                                               damage='melee x1.0 (STR 130)')],
               spawn='late-game overworld; cond 0x10095 / 0x100b4 (after flag 0x95 / 0xb4)', status='DISASM'),
    0x50: dict(name='Ninja Boss', archetype='ninja fighter',
               phases=[dict(states=['0x198'], trigger='spawn', pattern='idle until within 100 px'),
                       dict(states=['0x199'] + [hex(s) for s in range(0x19a, 0x1a2)], trigger='aggro',
                            pattern=['block check 0x1a1 (as generic: >90 f, player attacking 20%/f -> block)',
                                     'within 60 px: 9%/f NinjaTeleport (70..100 px jump)',
                                     'within 50 px: 12%/f melee (MeleeAttack + Wait 18)',
                                     'within 60 px: 8%/f step toward 1.0 px/f (ends 6%/f)',
                                     '70..130 px: 13%/f -> 0x19c align 1.0 px/f (ends 0.5%/f) + NinjaStarThrow '
                                     '+ Wait 90',
                                     'beyond 90 px: 5%/f chase 1.0 px/f (ends 2%/f)',
                                     '3%/f align 1.0 px/f; 0.8%/f back off 0.7969 px/f; else stand'])],
               attacks=[dict(name='punch', damage='melee sub 2 x1.0', recovery_frames=18),
                        dict(name='ninja star', projectile='ninja_star', fire_frame=8, end_frame=32,
                             post_wait_frames=90)],
               specials=['teleport (NinjaTeleport, VERIFIED jump (59,85))'],
               movement=dict(step=1.0, chase=1.0, align=1.0, back_off=0.7969),
               spawn='Ninja Stronghold (13,53) cond 1 (scripted encounter); rec extra script #398',
               verified=['star spawn +7 f after state entry, 1.75 px/f, energy x1.0', 'teleport jump'],
               status='VERIFIED/DISASM'),
    0x51: dict(name='Ninja Boss (variant)', archetype='disguised ninja',
               phases=[dict(states=['0x1a2'], trigger='spawn', pattern='NinjaDisguise (sits as a prop)'),
                       dict(states=['0x1a3', '0x1a4'], trigger='player within 60 px',
                            pattern='NinjaReveal: shakes 64 frames, smoke puff'),
                       dict(states=['0x1a5..0x1ad'], trigger='after reveal', pattern='same as Ninja Boss 0x50')],
               status='DISASM'),
    0x81: dict(name='Pharoah Totenhotep', archetype='caster (no melee)',
               phases=[dict(states=['0x2ec'], trigger='spawn', pattern='idle until within 100 px'),
                       dict(states=['0x2ed', '0x2ee', '0x2ef', '0x2f0', '0x2f1', '0x2f2', '0x2f3', '0x2f6'],
                            trigger='aggro (MarkTime starts the 1200-frame burst timer)',
                            pattern=['burst check 0x2f6: 1200 f since MarkTime -> burst phase; player attacking '
                                     'me: 40%/f -> step toward',
                                     'within 50 px: 8%/f step toward 0.3984 (ends 6%/f)',
                                     'within 30 px: 8%/f back off 0.5977 until beyond 65 px (or 0.4%/f)',
                                     '40..130 px and >90 f since the last shot (IfCooldown2Over 90): 13%/f -> '
                                     '0x2f0 align 0.3984 (ends 0.5%/f), MummyWrapShot, MarkTime2, Wait 24',
                                     'beyond 70 px: 5%/f chase 0.3984; 3%/f align 0.3984; else stand']),
                       dict(states=['0x2f4', '0x2f5'], trigger='1200 frames after the last burst phase',
                            pattern='0x2f4: 80% -> 0x2f5 MummyWrapBurst (8 wraps, 24 f) -> 0x2f4 again; 20% -> '
                                    '0x2ed (restart the 1200 f timer). Mean 4 bursts, one every 26 frames')],
               attacks=[dict(name='wrap shot', projectile='mummy_wrap', fire_frame=8, end_frame=24,
                             cooldown_frames=90, post_wait_frames=24),
                        dict(name='wrap burst', projectile='mummy_wrap', count=8, period_frames=26,
                             phase_interval_frames=1200)],
               movement=dict(step=0.3984, chase=0.3984, align=0.3984, back_off=0.5977),
               specials=['mummy wraps slow the player 48/256 px/f each (stack up to 8); mash d-pad x8 to escape'],
               spawn="Totenhotep's Chamber (14,24) cond 1 (scripted); rec extra script #420",
               verified=['burst = 8 wraps, period 26 f', 'slow -48 per wrap, escape on 8th press'],
               status='VERIFIED/DISASM'),
    0x8e: dict(name='Worm Head', archetype='segmented snake',
               phases=[dict(states=['0x318'], trigger='spawn', pattern='WormSpawnBody: 5 Worm Body (0x8F) behind '
                            'it (12 px apart)'),
                       dict(states=['0x319'], trigger='immediately', pattern='WormHeadMove forever')],
               attacks=[dict(name='contact', damage='melee sub 2 x1.0 (STR 35)', rehit_cooldown_frames=20)],
               movement=dict(speed=1.0, turn='0.5%/f 90-degree turn (left/right 50/50); turns when blocked'),
               specials=['segments are separate enemies (type 0x8F, 5170 HP each) that replay the head path '
                         '12 frames late; head resist melee 64 / energy 192'],
               spawn='Stomach (21,9/11/14) cond 0', verified=['5 segments', '1.0 px/f', 'follow lag ~12 f'],
               status='VERIFIED'),
    0x8f: dict(name='Worm Body', archetype='segment (spawned by Worm Head)',
               phases=[dict(states=['0x31a'], trigger='spawned by the head', pattern='WormBodyFollow')],
               attacks=[dict(name='contact', damage='melee x1.0', rehit_cooldown_frames=20)], status='VERIFIED'),
    0x90: dict(name='(unnamed worm part)', phases=[dict(states=['0x31b'], pattern='Idle')],
               notes='same stats block as the worm; never spawned by the head code; no spawn record. INFERRED unused',
               status='DISASM'),
    0x5b: dict(name='Pikkon', archetype='fighter with ranged options',
               phases=[dict(states=['0x1bd'], trigger='spawn', pattern='idle until within 72 px'),
                       dict(states=[hex(s) for s in range(0x1be, 0x1c8)], trigger='aggro',
                            pattern=['block check 0x1c7: >90 f in state; player attacking 20%/f or player beam '
                                     'line 80%/f -> block (generic block state)',
                                     'beyond 120 px -> 0x1bf chase 0.6992 until within 80 px (or 1%/f)',
                                     'beyond 50 px: 50%/f -> ranged: 70% 0x1c5 align 0.6992 + KiBlast, else '
                                     '0x1c4 align + EnergyBarrage',
                                     'close: 50% -> 0x1c0 (chase 0.6992; in melee range 5%/f punch '
                                     '(re-punch 10%), 2%/f step-in), 70% -> 0x1c2 step toward, else 0x1c3'])],
               attacks=[dict(name='punch', damage='melee x1.0', recovery_frames=18, chain_chance='10%'),
                        dict(name='ki blast', node='KiBlast', note='fires at frame 16, 3 px/f (AI worker)'),
                        dict(name='energy barrage', projectile='pikkon_barrage', frames=48,
                             damage='energy sub 4 x0.05 per hit, 1 hit/frame')],
               movement=dict(speed=0.6992), spawn='Other World Stadium (1,20) cond 1; rec extra script #102',
               verified=['barrage: 1 hit/frame, mult 51'], status='VERIFIED/DISASM'),
    0x26: dict(name='Goten (tag game)', archetype='story actor, harmless (all stats 0)',
               phases=[dict(states=['0xe6', '0xe7', '0xe8'], trigger='spawn',
                            pattern='repeat 10x: wait (30%/f), TauntAnim, face player; then Wait 120 and '
                                    'RunScript #121')],
               attacks=[], status='DISASM'),
}

for _t, _n in ((0x23, 'Gohan'), (0x25, 'Goten'), (0x27, 'Goten (HTC sparring)'), (0x28, 'Gotenks'),
               (0x2d, 'Hercule'), (0x31, 'Idasa'), (0x55, 'Olibu'), (0x5a, 'Piccolo'), (0x82, 'Trunks (HTC)'),
               (0x84, '(tournament opponent 0x84)'), (0x87, '(unnamed fighter 0x87)')):
    BOSSES[_t] = dict(name=_n, archetype='generic fighter', tree='GENERIC_FIGHTER',
                      attacks=[dict(name='punch', node='MeleeAttack', damage='melee sub 2 x1.0',
                                    recovery_frames=18, chance='12%/f within 50 px and in melee range')],
                      movement=dict(chase=0.5977, step=0.5977, back_off=0.7969),
                      specials=['blocks (20%/f when attacked after 90 f in state)'],
                      notes='melee only; differences between these opponents are stats only (enemies.json)',
                      status='DISASM')
BOSSES[0x3b] = dict(name='Human (tournament)', archetype='generic fighter without block',
                    tree='GENERIC_FIGHTER (states 0x1b2..0x1b8, block state = Idle)', status='DISASM')
BOSSES[0x54] = dict(name='(placeholder 0x54)', phases=[dict(states=['0x1b0'], pattern='Idle')],
                    notes='0 HP/0 stats; INFERRED unused/dummy', status='DISASM')
