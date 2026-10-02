"""Enemy projectile system + every projectile class used by regular enemies (machine-readable).

Units: positions/speeds in px and px/frame unless the key says raw (1/256 px), times in frames (60/s),
boxes as (ox, oy, w, h): hit box = (x - ox, y - oy) .. (+w, +h) around the object position.  Facing / dir
index: 0 down, 1 up, 2 left, 3 right.  Damage always goes through ComputeDamage 0x0800E9D8 (notes section 4)
with attacker = the owner enemy unless stated.  Evidence scripts: bv_regular_trace.py / _batch.py /
_analyze.py (79 regular enemy types force-spawned, player held at 3 offsets, 2400+ damage events to the
player all reproduced exactly by damage_model.py), bv_regular_vamp.py, bv_regular_bio.py, bv_regular_align.py."""

PROJECTILE_BASE = dict(
    status='VERIFIED (field use and movement traced; damage reproduced exactly)',
    ctor='0x08019B04 base(obj, gfx_id, owner): vtable, +0x0C sprite slot, +0x0E gfx id, +0x10 dir = owner+0x10, '
         '+0x11 = owner+0x1D0, +0x14 = 0, +0x1C = owner',
    box_update='0x08019B4E: x0 = (x>>8) - ox, y0 = (y>>8) - oy, x1 = x0 + w, y1 = y0 + h -> +0x28..+0x34; '
               '+0x04 = y>>8 (sort key). Box def = s8 ox, s8 oy, u8 w, u8 h, u32 OAM attrs, ptr gfx (12 bytes)',
    fields={'+0x18': 'box/frame def pointer (table + 12*dir, stepped +-0x30 for animation)',
            '+0x1C': 'owner enemy (cleared by message 0x11/0x12 when the owner dies -> no more damage)',
            '+0x20/+0x24': 'position, 24.8 fixed point',
            '+0x28..+0x34': 'hit box x0,y0,x1,y1 (px)',
            '+0x40/+0x44': 'velocity (1/256 px per frame)',
            '+0x48': 'frame counter'},
    spawning='controller frame N: needs 0x080116FC(world) >= 0x80 (pool room; some 0x100/0x400/0x700/0x1400), '
             'builder(0, &muzzle_xy, dir, owner) then 0x0801191A(world 0x03001C4C, obj). muzzle = enemy pixel '
             'position + per-dir (dx,dy) from the spawn-data block',
    removal='no lifetime timer for straight shots: removed when the box leaves the camera rect at 0x030019BC '
            '(240x160 view), explodes when any 8x8 tile under the box is blocked in the projectile passability '
            'bitmap [0x03001A50] (0x08009324) or the box leaves 0..2048, and explodes/vanishes when a hit lands',
    hit_message='0x08011C70(world, 0, msg) / 0x08011C0C / 0x08011CCA: msg+0 u8 kind (0 attack, 2 explosion), '
                '+4..+0x10 box, +0x14 hit struct {u8 dir, u32 mult (0x400=1.0), attack type ptr, attacker=owner}, '
                '+0x30 sender. The player handler runs ComputeDamage(player, hit)',
    vs_player_attacks='bullets, rifle shots, shuriken, tank shells: a player attack message (kind 0) whose box '
                      'overlaps destroys them (their vfunc +0x24) - INFERRED from code; energy waves only when the '
                      'attack comes head-on (msg dir == wave dir ^ 1) and overlaps the head +-12 px',
    block='normal block rule: R held and (hit dir ^ 1) == player facing -> damage x 1/16 (0x1000>>16)',
    iframes='during the player hurt state (24 frames) resist = 0 -> damage 0, the projectile is still consumed',
    enemy_melee=dict(
        status='VERIFIED (14/14 Majin Fighter hits on the first frame of the punch controller; 2000+ exact)',
        hit_fn='0x0802A0C4 (copies: 0x08033BC4, 0x0803CBD0 path, 0x080361B8)',
        timing='MeleeAttack switches to controller 0x03001B50 (16-frame punch); hit test once on frame 0',
        requires='player obj+0x1CC != 0 (hittable)',
        facing='turns to face the player on the dominant axis (|dx|>|dy| -> left/right)',
        box={'right': '(cx, cy) .. (B.x1+3, B.y1+3)', 'left': '(B.x0-4, cy) .. (cx, B.y1+3)',
             'down': '(B.x0, B.y0) .. (B.x1, B.y1-6) with B = pos..pos+(w,h) at obj+0x19C/0x19D',
             'up': '(B.x0, B.y0) .. (B.x1, cy)'},
        box_note='B = enemy collision box from 0x0800FE78, (cx,cy) its centre',
        damage='attack type 0x086B8098 (melee sub 2), multiplier 0x400 (HeavyPunch 0x600), attacker STR'),
)

PROJECTILES = {
    'gun_bullet': dict(
        status='VERIFIED', ctor='0x080274B4', vtable='0x08049F0C', update='0x08027562', message='0x0802764E',
        node='GunShot 0x080265E9', spawn_data='0x0874CA38', fire_frame=4, controller_frames=16,
        muzzle_by_dir=[(4, 4), (13, -24), (-8, -8), (24, -8)],
        speed_px_per_frame=2.625, velocity_raw_by_dir=[(0, 672), (0, -672), (-672, 0), (672, 0)],
        direction='straight along the facing (4-way, enemy turns to face the player first)',
        box_by_dir=[(3, 10, 8, 16), (3, 5, 8, 16), (5, 4, 16, 8), (10, 4, 16, 8)], box_table='0x0874E968',
        lifetime='until off-screen / wall / hit (measured 25-28 f at 60 px)',
        on_end='explosion effect 0x0802A81C(anim 0x08712FDC, 32 f, no damage)',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400, stat='owner POW vs END2',
                    note='VERIFIED e.g. Gunman POW 13 -> Goku END2 26: 14340/1024 HP'),
        users=['0x02 Assassin', '0x2C Gunman', '0x6C Sniper']),
    'rifle_bullet': dict(
        status='VERIFIED', ctor='0x080276A8', vtable='0x08049FC0', update='0x0802776E', message='0x080278E2',
        node='RifleShot 0x08026869', spawn_data='0x0874CA9C', fire_frame=8, controller_frames=32,
        muzzle_by_dir=[(4, 4), (13, -24), (-8, -8), (24, -8)],
        speed_px_per_frame=1.75, velocity_raw_by_dir=[(0, 448), (0, -448), (-448, 0), (448, 0)],
        direction='straight along the facing', box=(8, 14, 16, 16), box_table='0x0874E614 (16x16, offset per dir)',
        lifetime='until off-screen / wall / hit (23-26 f at 60 px)', trail='smoke effect every 4 frames',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400, stat='owner POW vs END2'),
        users=['0x2E Hessian', '0x47 Marauder', '0x4B Mercenary']),
    'energy_wave': dict(
        status='VERIFIED (speed/lifetime/damage) / DISASM (shape)', ctor='0x0802A2EC', vtable='0x08049C98',
        update='0x0802A3D2', message='0x0802A704',
        nodes=['SoldierThrow 0x08026739 (fire f8, controller waits for the wave, min 32 f)',
               'TurretFire 0x0802B6E5 (fire f8, 72 f)', 'CyborgShot 0x0802F709 (fire f1, min 16 f)'],
        spawn_data={'SoldierThrow': '0x0874CA6C', 'TurretFire': '0x08752E78', 'CyborgShot': '0x08757A80'},
        muzzle_by_dir={'SoldierThrow/CyborgShot': [(2, 8), (9, -28), (-16, -11), (24, -11)],
                       'TurretFire': [(4, 4), (4, -25), (-13, -14), (22, -13)]},
        shape='a segment from tail (pos, px) to head = pos + dir*len; len starts 8 and grows 4/frame to 48 '
              '(12 frames), then the whole segment moves 4 px/frame. Hit box = segment bounding box +-4 px',
        speed_px_per_frame=4, direction='straight along the facing, unit vectors table 0x0807268C',
        lifetime='mode 0 max 240 frames (+0x1C > 0xF0); on hit / wall / timeout -> mode 1: tail catches up 4 '
                 'px/frame (len -= 4) and the object is removed at len <= 0',
        on_hit='spark effect 0x0802A81C(anim 0x086FD9D0, rate 0x1000+rand(0x1000)); enters mode 1',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400, stat='owner (+0x4C) POW vs END2',
                    sent_with='0x08011C0C'),
        users=['0x03 Atomizer', '0x32 Ion Cannon', '0x40 Laser Turret', '0x11 Cyborg', '0x30 Hyper Cyborg',
               '0x4A Mega Cyborg', '0x45 Majin Soldier', '0x16 Elite Majin Soldier', '0x7C Super Majin Soldier',
               '0x67 Majin Shield Soldier', '0x17 Elite Shield Soldier', '0x7D Super Shield Soldier']),
    'flame': dict(
        status='VERIFIED', ctor='0x080246B4', emitter='0x080249E0', vtable='0x0804BFCC', update='0x080247BE',
        node='FlameBreath 0x08024025', per_frame='one flame per frame for frames 4..127 of the 128-frame action',
        nozzle_by_dir=[(46, 10), (16, -48), (-6, -12), (74, -12)],
        speed_px_per_frame='2.0 along the facing + uniform jitter rand(192)-96 raw per axis (+-0.375 px/f)',
        velocity_raw_by_dir=[(0, 512), (0, -512), (-512, 0), (512, 0)], box='16x16 centred (pos +-8)',
        lifetime=20, hits='at most once per flame (+0x40 flag); direct ComputeDamage call, not a message',
        damage=dict(attack_type='0x086B80A8', kind='energy', sub=4, mult=51, mult_note='int(51.2f) = 0.05x',
                    stat='owner POW vs END2', note='VERIFIED 372/372 exact'),
        after_hit='message 0x17 to the player (knockback/hurt request)',
        users=['0x01 Annihilator', '0x13 Death Machine', '0x14 Destroyer']),
    'bomb': dict(
        status='VERIFIED (lifetime 71 f, damage) / DISASM (arc)', ctor='0x0801C6E4', vtable='0x0804A2E8',
        update='0x0801C80A', node='BombThrow 0x08026B2D', spawn_data='0x0874CAD0', throw_frame=16,
        muzzle_by_dir=[(12, -8), (12, -8), (16, -16), (0, -16)],
        flight='velocity v = clamp(player-enemy, +-128 px, facing axis only) * 0x71D >> 8 raw; vy starts at '
               'v.y - 0x200 and gains +0x20 per frame (gravity); height z = 0x800 + vz with vz from 0x200 '
               'decreasing 0x20/frame; lands when z < -0xC00 (~40 frames)',
        fuse='30 frames (blinking), then explodes', lifetime=71,
        explosion='one message kind 2 with the bomb box (16x16 centred, 0x0871E984) to world 0x0871E200+4 '
                  '(0x08011C0C), then 0x0801C750 spawns 4 debris (0x08049F38, 32 f) + a blast effect (0x0804A2C0) - '
                  'cosmetic (INFERRED: no hit sends)',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400, stat='owner POW vs END2',
                    note='VERIFIED 15/15 exact'),
        users=['0x08 Bomber', '0x2B Grenadier', '0x43 Mad Bomber']),
    'tank_shell': dict(
        status='VERIFIED', ctor='0x0801C900', vtable='0x0804A368', update='0x0801C9C4', node='TankShoot 0x0802E2F3',
        spawn_data='0x08757350', speed_px_per_frame=1.75, velocity_raw_by_dir=[(0, 448), (0, -448), (-448, 0), (448, 0)],
        box=(8, 14, 16, 16), box_table='0x0871E630', trail='smoke every 4 frames',
        on_end='explosion 0x0801C978', damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400,
                                                     stat='owner POW vs END2', note='VERIFIED 30/30 exact'),
        users=['0x3A Juggernaut', '0x7F Tank', '0x8C War Machine']),
    'shuriken': dict(
        status='VERIFIED', ctor='0x0802C790', vtable='0x0804BAA0', update='0x0802C7EA', message='0x0802C8AE',
        node='0x0802C18F family (ninja throw; node owned by the bosses/others group)', spawn_site='0x0802C116',
        speed_px_per_frame=1.75, velocity_raw_by_dir=[(0, 448), (0, -448), (-448, 0), (448, 0)],
        box=(4, 4, 8, 8), anim='2 frames, swap every 8 frames', on_hit='removed (no explosion)',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400, stat='owner POW vs END2',
                    note='VERIFIED 31/31 exact'),
        users=['0x4E Ninja', '0x65/0x66 Shadow Warrior', '0x68/0x69 Shinobi']),
    'mummy_bandage': dict(
        status='VERIFIED (flight, damage) / INFERRED (wrap effect)', ctor='0x0802D79C', vtable='0x0804A830',
        update='0x0802D940', node='0x0802D1CD (owned by the others group)', spawn_site='0x0802D16A',
        speed_px_per_frame=1.75, velocity_raw_by_dir={'straight': [(0, 448), (0, -448), (-448, 0), (448, 0)],
                                                      'diagonal': [(-316, 316), (316, -316), (-316, -316), (316, 316)]},
        box_by_axis=[(8, 4, 16, 8), (4, 8, 8, 16)], box_table='0x087571F4',
        on_hit='damage, then message 0x25 counts attached bandages (0x08011CCA); if < 8, after 4 frames the bandage '
               'sticks to the player at slot offset table 0x08756BB4 (global slot counter 0x030030BC, 8 slots) and '
               'follows the player; it is shaken off by d-pad wiggling (0x08040284(mask 0xF0, 8, 60)). Measured: '
               'stuck bandages lived 212-401 frames with no input',
        damage=dict(attack_type='0x086B8090', kind='energy', sub=1, mult=0x400, stat='owner POW vs END2',
                    note='VERIFIED 18/18 exact'),
        users=['0x10 Cursed One', '0x4D Mummy', '0x59 Pharoah']),
    'grapple_arm': dict(
        status='INFERRED (spawn timing VERIFIED)', ctor='0x08037EA8', vtable='0x0804B380', update='0x08037FB4',
        node='GrappleArm 0x08037E8D', spawn_data='0x0876CCB0', fire_frame=28,
        shape='extending arm like energy_wave (start len 8, unit vectors 0x0876CCE8), max reach 0x50 = 80 px',
        damage=dict(attack_type='0x086B8088', kind='melee', sub=0, mult=0x400, stat='owner STR (can crit)'),
        on_hit='player controller -> 0x03001B00 (held) and pulled toward the Bio Mech',
        users=['0x05 Bio Mech', '0x71 Super Bio Mech', '0x83 Ultra Bio Mech']),
}

# grab / contact mechanics that are not projectiles
SPECIALS = {
    'vampire_grab': 'see boss_nodes_regular VampireGrab: POW HP drain every 16 frames + equal self-heal; escape = 9 '
                    'inputs with gaps <= 10 frames (VERIFIED drain)',
    'tank_crush': 'IfContactHit every frame: standard melee hit x1.0 STR (VERIFIED)',
    'heavy_punch': 'Bruiser/Goon/Thug: 48-frame wind-up, x1.5 melee (VERIFIED)',
    'shield_soldier': 'shield up/down at random (2%/5% per frame); 5th counter tick breaks the shield -> plain '
                      'fighter tree 0x153; the shield graphics carry no stat change (block mechanism INFERRED)',
}

BOSSES = {}
