"""Decoders for LoG1 ROM tables (shared by the extract_*.py scripts).
All addresses are GBA bus addresses (ROM offset = addr - 0x08000000)."""
import json
from common import *

# ---- table addresses (see notes.md for the code that reads each one)
SPRITE_NAME_PTRS = 0x080109F0     # u32[109] -> ASCII sprite/actor names ("armyguy1".."yamcha")
SPRITE_TO_TEMPLATE = 0x0815A69C   # u8[109]  sprite id -> enemy template id (0 = none/squirrel)
SPRITE_TO_DEF = 0x0815A709        # u8[109]  sprite id -> index into SPRITE_DEF_PTRS
SPRITE_DEF_PTRS = 0x085C0C80      # u32[]    -> sprite definition (0x2C bytes)
ENEMY_TEMPLATES = 0x086B6094      # 0x40-byte records, ids 0..0x21 (init_char 0x08006D90 rejects >0x22)
N_TEMPLATES = 0x22
LEVEL_TABLE = 0x080114A8          # 16-byte records {exp, maxHP, maxKi, flightMax}, terminated by exp==0
SPEED_TABLE = 0x080116A4          # u16[] speed class -> 1/16 px per frame (enemy char+0x34 index)
MAP_TABLE = 0x081CA09C            # u32[42] -> map record (0x34 bytes)
N_MAPS = 42
ITEM_TABLE = 0x085C224C           # 24-byte item records, ids 0..33
ITEM_MSG_PTRS = 0x085C2FB8        # u32[] pickup message per item id
MOVE_NAME_PTRS = 0x08010000       # u32[3] -> "Ki Blast", "Kame Hame Ha", "Solar Flare"
MOVE_SPRITE_DEFS = 0x08011E38     # u32[3] projectile sprite def per player move
MOVE_STATE_TABLES = 0x08011E44    # u32[3] projectile entity state tables
MOVE_SPAWN_OFFSETS = 0x08011E50   # s16[4][3] {dx, dy, entity+0x0C flags (OAM priority bits 4-5)} per facing, used by 0x0800B3FC
KIBLAST_FLY_TABLE = 0x08011E68    # s16[4][4] {dx, dy, vx, vy} per facing (0x0800B554)
KAME_GROW_TABLE = 0x08011E88      # s16[4][4] {dx,dy,dw,dh} per facing while extending (0x0800BCD8)
STATE_ANIM_TABLE = 0x086B6068     # u32[8] char state -> entity animation index (0x08007F4C)
ENEMY_PROJ_STATE_TABLES = 0x0801206C

FACINGS = ['down', 'up', 'left', 'right']
ANIM_NAMES = ['Standing', 'Walking', 'Death', 'Melee1', 'Melee2', 'Ki', 'Excited', 'Sad', 'Transform',
              'SSaiyaJin', 'Monkey', 'Monster', 'Raditz', 'Bubbles', 'Flying', 'Bored', 'Mallet']
CHAR_STATES = {0: 'idle', 1: 'walk (ground)', 2: 'unused/excited', 3: 'move (flying layer)',
               4: 'ki attack', 5: 'melee attack', 6: 'hurt', 7: 'dead'}


def sprite_names():
    return [cstr(u32(SPRITE_NAME_PTRS + 4 * i)) for i in range(109)]


def h(v):
    return f'0x{v:08X}'


_anim_tables = None


def anim_table_rows(anim):
    """Number of 16-byte rows in an animation table, bounded by the next table in ROM."""
    global _anim_tables
    if _anim_tables is None:
        tabs = set()
        for i in range(0, 0x80):
            d = u32(SPRITE_DEF_PTRS + 4 * i)
            if not (0x08000000 <= d < 0x08800000):
                break
            tabs.add(u32(d + 0x28))
        for p in (0x086B3AA8, 0x086B3AD4, 0x086B3C50, 0x086B376C, 0x086B3FFC, 0x086B3400):
            tabs.add(u32(p + 0x28))
        _anim_tables = sorted(tabs)
    nxt = [t for t in _anim_tables if t > anim]
    return (nxt[0] - anim) // 16 if nxt else len(ANIM_NAMES)


def sprite_def(idx):
    """Sprite definition (0x2C bytes): footprint size, per-facing hurtbox, palette, anim table."""
    d = u32(SPRITE_DEF_PTRS + 4 * idx)
    boxes = {}
    for f in range(4):
        a = d + 4 + 8 * f
        boxes[FACINGS[f]] = dict(x0=s16(a), y0=s16(a + 2), x1=s16(a + 4), y1=s16(a + 6))
    anim = u32(d + 0x28)
    states = []
    n_rows = min(len(ANIM_NAMES), anim_table_rows(anim))
    for st in range(n_rows):
        ptrs = [u32(anim + 16 * st + 4 * j) for j in range(4)]
        if all(0x08000000 <= p < 0x08800000 for p in ptrs):
            states.append(st)
    return dict(def_addr=h(d), footprint=dict(w=u16(d), h=u16(d + 2)), hurtbox=boxes,
                palette_ptr=h(u32(d + 0x24)), anim_table=h(anim),
                anims_present=[ANIM_NAMES[s] for s in states])


def speed_class(i):
    return u16(SPEED_TABLE + 2 * i)


def template(i):
    a = ENEMY_TEMPLATES + 0x40 * i
    t = {
        'id': i, 'rom_addr': h(a), 'name': cstr(u32(a + 4)),
        'raw': rom()[ra(a):ra(a) + 0x40].hex(),
        'field_08_char02': s32(a + 0x08),
        'exp_reward': s32(a + 0x0C),
        'move_type': s16(a + 0x10),        # -> char+0x3C
        'target_type': s16(a + 0x12),      # -> char+0x3E
        'fight_style': s16(a + 0x14),      # -> char+0x40
        'field_16': s16(a + 0x16),         # -> char+0x42 (not read by generic AI)
        'group_flag': s16(a + 0x18),       # -> char+0x44 (>=2: char+0x90=self, char+0x98=1)
        'flying': s16(a + 0x1A),           # -> char+0x46 (1 = uses flying collision layer)
        'aggro_tiles': s16(a + 0x1C),      # -> char+0x48
        'leash_home_x': s16(a + 0x1E), 'leash_home_y': s16(a + 0x20), 'leash_radius': s16(a + 0x22),
        'guard_x': s16(a + 0x24), 'guard_y': s16(a + 0x26),
        'speed_class': s16(a + 0x28),      # -> char+0x34 ; speed = SPEED_TABLE[class]
        'field_2a_char78': s16(a + 0x2A), 'field_2c_char7c': s16(a + 0x2C),
        'field_2e': s16(a + 0x2E), 'field_30': s16(a + 0x30),
        'ki': s32(a + 0x34), 'hp': s32(a + 0x38), 'defense': s32(a + 0x3C),
    }
    return t


def level_table():
    out = []
    i = 0
    while True:
        a = LEVEL_TABLE + 16 * i
        exp, hp, ki, fly = s32(a), s32(a + 4), s32(a + 8), s32(a + 12)
        if exp == 0:
            break
        out.append(dict(level=i + 1, exp_required=max(exp, 0), raw_exp_field=exp, max_hp=hp, max_ki=ki,
                        max_flight_energy=fly, rom_addr=h(a)))
        i += 1
    return out


def map_record(m):
    rec = u32(MAP_TABLE + 4 * m)
    c = u32(rec + 0x14)
    return rec, c


def dump(name, obj):
    p = DATA / name
    p.write_text(json.dumps(obj, indent=1))
    print('wrote', p)
