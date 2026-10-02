"""Readers for every Buu's Fury data table decoded so far (all addresses are GBA ROM addresses).

Everything here reads the ROM directly; no emulator needed (except the optional script decompressor).
"""
from common import *
from strtab import S

# ----------------------------------------------------------------------------- addresses
STRTAB = 0x0806BCE8          # u32 ptr[1534] -> UTF-16LE strings (string ids used by all tables)
CHAR_FORMS = 0x086B82EC      # 54 x 24 bytes: playable character forms
N_FORMS = 54
GROWTH = 0x086B72E0          # 9 x 32 bytes (indexed by character index 0..8; 5..8 all-zero)
XP_TABLE = 0x086B7084        # u32[151]: index i -> total XP for level 50+i (t[0] = 0)
INIT_RECORDS = 0x086FDA4C    # 9 x 18 bytes: new-game character records
SKILLS = 0x0871DAF8          # 24 x 24 bytes: energy skills/specials
SKILL_COSTS = 0x0871DA98     # cost bytes/words used by skill canUse() functions
ATTACK_TYPES = 0x086B8088    # 18 x 8 bytes: damage-type descriptors {u8 kind, u8 sub, u16 0, u32 power}
EQUIPMENT = 0x086F82EC       # 162 x 0x2C bytes
N_EQUIP = 162
SUPPLIES = 0x083BD2F8        # 83 x 0x34 bytes: supplies + story items
N_SUPPLIES = 83
ENEMY_TYPES = 0x08711274     # 150 x 28 bytes
N_ENEMY = 0x96
ENEMY_DB = 0x0874D380        # 232 x 16 bytes: Capsule Corp database (scouter) {name, desc, gfx, portrait, type}
N_DB = 232
AI_STATES = 0x0870F514       # u32 pairs {behaviour def*, aux*} indexed by AI state id
GFX_TABLE = 0x086B6BB4       # u32 ptr[] indexed by graphics id (character sprite definitions)
AREAS = 0x0808E2E0           # 0x1C4 x 0x38 bytes
N_AREAS = 0x1C4
SCRIPTS = 0x0879B704         # u32 ptr[522] -> script headers (bytecode)
MELEE_RECT = 0x083C344C      # 4 dirs x {x0,y0,x1,y1} s32 melee hitbox relative to player pos
TALK_RECT = 0x083C340C       # 4 dirs x {x0,y0,x1,y1} interaction probe
DIR_VEC = 0x08719C40         # 4 dirs x {vx,vy} s32 (24.8) ki-blast velocity at speed 1 (=3 px/frame)
COIN_DEFS = 0x086FC250       # 4 ptr -> coin pickup defs (value at +0xC)


def form(i):
    a = CHAR_FORMS + 24 * i
    b = rom()[ra(a):ra(a) + 24]
    f0, nm, NM, ptr, ci, cj, gfx = struct.unpack_from('<HHIIBBH', b)
    return dict(index=i, address=hex(a), flags=f0, name_string=nm, name=S(nm), upper_name=S(NM),
                controller_slot=hex(ptr), char_index=ci, unk_0d=cj, gfx_id=gfx,
                portrait_or_anim_bytes=list(b[16:22]), transform_up=b[22], transform_down=b[23])


def growth(ci):
    a = GROWTH + 32 * ci
    return [u32(a + 4 * k) for k in range(8)]


def xp_for_level(level):
    """Total XP needed to *be* at `level` (50..200)."""
    return u32(XP_TABLE + 4 * (level - 50))


def init_record(ci):
    a = INIT_RECORDS + 18 * ci
    b = rom()[ra(a):ra(a) + 18]
    n = b[11]
    return dict(level=b[0], hp=struct.unpack_from('<H', b, 2)[0], ep=struct.unpack_from('<H', b, 4)[0],
                STR=b[6], POW=b[7], END=b[8], SPD=b[9], form=b[10], skills=list(b[12:12 + n]))


def skill(i):
    a = SKILLS + 24 * i
    mx, ic1, ic2, nm, ds, f1, f2 = struct.unpack_from('<IIIHHII', rom(), ra(a))
    return dict(id=i, address=hex(a), name=S(nm), description=S(ds), max_level=mx if mx != 0xff else None,
                can_use_fn=hex(f1), activate_fn=hex(f2), icon=hex(ic1), icon2=hex(ic2))


def attack_type(i):
    a = ATTACK_TYPES + 8 * i
    return dict(sub=i, address=hex(a), kind=u8(a), kind_name='melee' if u8(a) == 0 else 'energy',
                id_byte=u8(a + 1), power=u32(a + 4))


def equipment(i):
    a = EQUIPMENT + 0x2c * i
    b = rom()[ra(a):ra(a) + 0x2c]
    nm, ds = struct.unpack_from('<HH', b)
    stats = struct.unpack_from('<5b', b, 0x16)
    price = struct.unpack_from('<i', b, 0x20)[0]
    return dict(id=i, address=hex(a), name=S(nm), description=S(ds), slot=['body', 'hands', 'feet', 'accessory'][b[0x14]],
                required_level=b[0x15], STR=stats[0], POW=stats[1], END=stats[2], END_energy=stats[3], SPD=stats[4],
                byte_1b=b[0x1b], exp_bonus_256ths=struct.unpack_from('<I', b, 0x1c)[0], price=price,
                flags_0x24=struct.unpack_from('<I', b, 0x24)[0], effect_script=hex(struct.unpack_from('<I', b, 0x28)[0]),
                icon=hex(struct.unpack_from('<I', b, 0x10)[0]), raw=b.hex())


def supply(i):
    a = SUPPLIES + 0x34 * i
    f0, sn, nm, pl = struct.unpack_from('<HHHH', rom(), ra(a))
    return dict(id=i, address=hex(a), story_item=bool(f0), short_name=S(sn), name=S(nm), plural=S(pl),
                description=S(u32(a + 8)), price=s32(a + 0x2c), use_script=hex(u32(a + 0x30)), icon=hex(u32(a + 0x18)))


def enemy_type(t):
    a = ENEMY_TYPES + 28 * t
    w = struct.unpack_from('<7I', rom(), ra(a))
    b = rom()[ra(a):ra(a) + 28]
    return dict(type=t, address=hex(a), exp=w[0], hp=w[1], ai_state=w[2], level_byte=b[0xc], STR=b[0xd], POW=b[0xe],
                END=b[0xf], resist_melee=b[0x10], resist_energy=b[0x11], byte_12=b[0x12], byte_13=b[0x13],
                hit_reaction=hex(w[5]), drop_script=hex(w[6]))


def db_entries():
    out = []
    for i in range(N_DB):
        a = ENEMY_DB + 16 * i
        nm, ds, x, y, z = struct.unpack_from('<HHIIi', rom(), ra(a))
        out.append(dict(index=i, name=S(nm), description=S(ds), gfx_id=x, portrait=y if y != 0xffffffff else -1, link=z))
    return out


def rect(base, d):
    a = base + 16 * d
    return [s32(a + 4 * k) for k in range(4)]
