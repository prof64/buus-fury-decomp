//! Persistent game state (the GBA's GameData block at `0x03001068` and its neighbours): story flags, the script
//! words, inventory (supply counts, owned equipment, zenie), the nine character records with their skills, skill
//! levels, equipment and forms, the party roster, the quest journal and the play time. Plain data inside `World`
//! (`World.story`), shared by every player (co-op partners play characters of the same party), snapshotted and
//! hashed with the world. Layout and limits: re/buu/notes.md "Field: triggers, breakables, pickups, game state"
//! and data/field/gamestate_layout.json.
//!
//! The active fighters (`Body::Player`) own the live copy of their character's record while they are in the world;
//! `sync_from_players` writes it back every frame (cheap: a few words), so `Story` is always the full save state.

use crate::content::GameData;
use crate::progression::{new_game_record, Record};
use crate::world::{Body, World};
use serde::{Deserialize, Serialize};

/// story flag bytes: GameData +0x55 .. +0xE3 (flag f = bit f & 7 of byte f >> 3)
pub const FLAG_BYTES: usize = 0x8F;
/// characters with a record (0x03001560 + 0x68 i): Goku, Gohan, Goten, Trunks, Vegeta, Gotenks, Vegito, Gogeta, other
pub const CHARS: usize = 9;
/// supply count cap (u8 counts at GameData +2, 83 supplies)
pub const SUPPLY_MAX: u8 = 255;
/// supply slots
pub const SUPPLIES: usize = 83;
/// equipment list capacity (GameData +0xE4: count + u32[250])
pub const EQUIPMENT_MAX: usize = 250;
/// zenie (u32 at GameData +0x4F4; no cap decoded)
pub const ZENIE_MAX: u32 = u32::MAX;

/// one character's persistent record (0x68 bytes in the game)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharState {
    pub rec: Record,
    /// current form (+0x14)
    pub form: i32,
    /// known skills in menu order (+0x0C count, +0x0D ids)
    pub skills: Vec<i32>,
    /// level per known skill (+0x2E..), parallel to `skills`
    pub skill_levels: Vec<i32>,
    /// selected skill (+0x15)
    pub sel: i32,
    /// equipment slots body, hands, feet, accessory (+0x3C..+0x48)
    pub equip: [i32; 4],
    /// SS cooldown (+0x04)
    pub ss_cooldown: i32,
    /// character flags (party membership bit 0, script flag 2 bit 1; `join_party` / `set_char_flag2`)
    pub flags: u8,
    /// the level chosen in the Skills popup per known skill (record +0x2E + 2i; `skill_levels` is +0x2F + 2i, the
    /// learned level, which `set_skill_level` sets both to); empty / missing entries = the learned level
    #[serde(default)]
    pub skill_cur: Vec<i32>,
}

impl CharState {
    pub fn in_party(&self) -> bool {
        self.flags & 1 != 0
    }
    /// the chosen level of the k-th known skill (record +0x2E + 2k)
    pub fn skill_cur(&self, k: usize) -> i32 {
        self.skill_cur.get(k).copied().unwrap_or_else(|| self.skill_levels.get(k).copied().unwrap_or(0))
    }
    pub fn skill_level(&self, skill: i32) -> i32 {
        self.skills.iter().position(|&s| s == skill).and_then(|k| self.skill_levels.get(k).copied()).unwrap_or(0)
    }
}

/// one quest log entry state (40 quests; the conditions themselves are scripts, see docs/script-host.md §5)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Quest {
    pub state: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Story {
    pub flags: Vec<u8>,
    /// script words L0..L3 (0x03001020)
    pub l: [i32; 4],
    /// script globals G0..G3 (0x03001058; the VM's shared globals, kept here so they snapshot with the world)
    pub g: [i32; 4],
    /// GameData game bits (`set_game_bit_20` / `_40`, `set_area_flag_c`)
    pub game_bits: u32,
    /// `get_variant_byte`
    pub variant_byte: i32,
    /// supply counts by supply index (GameData +2, u8[83])
    pub supplies: Vec<u8>,
    /// the equipment list (GameData +0xE4, acquisition order, one entry per copy; the game marks equipped entries in
    /// the high nibble, here the characters' `equip` slots say who wears what)
    pub equipment: Vec<i32>,
    pub zenie: u32,
    pub chars: Vec<CharState>,
    /// current (leader's) character index (GameData +0)
    pub current: i32,
    pub quests: Vec<Quest>,
    /// play time in frames (counted while the field runs)
    pub play_frames: u64,
}

impl Default for Story {
    fn default() -> Self {
        Story {
            flags: vec![0; FLAG_BYTES],
            l: [0; 4],
            g: [0; 4],
            game_bits: 0,
            variant_byte: 0,
            supplies: Vec::new(),
            equipment: Vec::new(),
            zenie: 0,
            chars: vec![CharState::default(); CHARS],
            current: 0,
            quests: vec![Quest::default(); 40],
            play_frames: 0,
        }
    }
}

impl Story {
    /// a new game: every character's new-game record (characters.json `new_game`), Goku in the party and current
    pub fn new_game(d: &GameData) -> Story {
        let mut s = Story::default();
        let n_sup = d.field.as_ref().map(|f| f.supplies.iter().map(|x| x.index + 1).max().unwrap_or(0)).unwrap_or(0) as usize;
        s.supplies = vec![0; n_sup.max(SUPPLIES)];
        // mods may add story characters after the game's nine (characters/*.json `index` 9+)
        let n = d.chars.iter().map(|c| c.index + 1).max().unwrap_or(0).max(CHARS as i32) as usize;
        s.chars.resize(n.min(64), CharState::default());
        for c in &d.chars {
            if c.index < 0 || c.index as usize >= s.chars.len() {
                continue;
            }
            s.chars[c.index as usize] = CharState {
                rec: new_game_record(c),
                form: c.form,
                skill_levels: vec![0; c.skills.len()],
                skills: c.skills.clone(),
                sel: 0,
                equip: c.equip.unwrap_or([0, 44, 77, 112]),
                ss_cooldown: 0,
                flags: c.in_party as u8,
                skill_cur: Vec::new(),
            };
        }
        s.chars[0].flags |= 1;
        s
    }

    pub fn flag(&self, f: i32) -> bool {
        if f < 0 {
            return false;
        }
        self.flags.get((f >> 3) as usize).is_some_and(|b| b & (1 << (f & 7)) != 0)
    }
    pub fn set_flag(&mut self, f: i32, on: bool) {
        if f < 0 {
            return;
        }
        let i = (f >> 3) as usize;
        if i >= self.flags.len() {
            self.flags.resize(i + 1, 0);
        }
        if on {
            self.flags[i] |= 1 << (f & 7);
        } else {
            self.flags[i] &= !(1 << (f & 7));
        }
    }

    pub fn supply(&self, item: i32) -> i32 {
        if item < 0 {
            return 0;
        }
        self.supplies.get(item as usize).copied().unwrap_or(0) as i32
    }
    /// add n (negative takes), clamped to 0..=255
    pub fn add_supply(&mut self, item: i32, n: i32) {
        if item < 0 {
            return;
        }
        let i = item as usize;
        if i >= self.supplies.len() {
            self.supplies.resize(i + 1, 0);
        }
        self.supplies[i] = (self.supplies[i] as i32 + n).clamp(0, SUPPLY_MAX as i32) as u8;
    }
    pub fn add_zenie(&mut self, n: i64) {
        self.zenie = (self.zenie as i64 + n).clamp(0, ZENIE_MAX as i64) as u32;
    }
    /// copies owned (unequipped) + equipped by any character
    pub fn equipment_count(&self, id: i32) -> i32 {
        self.equipment.iter().filter(|&&e| e == id).count() as i32
    }
    pub fn add_equipment(&mut self, id: i32) {
        if self.equipment.len() < EQUIPMENT_MAX {
            self.equipment.push(id);
        }
    }
    pub fn take_equipment(&mut self, id: i32) -> bool {
        match self.equipment.iter().position(|&e| e == id) {
            Some(k) => {
                self.equipment.remove(k);
                true
            }
            None => false,
        }
    }
    pub fn party(&self) -> Vec<i32> {
        (0..self.chars.len() as i32).filter(|&c| self.chars[c as usize].in_party()).collect()
    }
}

/// character index of a form
pub fn char_of_form(d: &GameData, form: i32) -> Option<i32> {
    d.form(form).map(|f| f.char_)
}

/// write every active fighter's live record back into its character's persistent record
pub fn sync_from_players(w: &mut World) {
    for &id in &w.players {
        let Some(i) = w.find(id) else { continue };
        let Body::Player(p) = &w.ents[i].body else { continue };
        let Some(c) = char_of_form(&w.data, p.form) else { continue };
        if c < 0 || c as usize >= w.story.chars.len() {
            continue;
        }
        let cs = &mut w.story.chars[c as usize];
        if cs.rec != p.rec {
            cs.rec = p.rec.clone();
        }
        cs.form = p.form;
        cs.equip = p.equip;
        cs.ss_cooldown = p.ss_cooldown;
        cs.sel = p.sel as i32;
        if cs.skills != p.skills {
            cs.skills = p.skills.clone();
            cs.skill_levels.resize(cs.skills.len(), 0);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Condition scripts: the area loader evaluates spawn conditions synchronously (`0x08004784`). The 115 condition
// scripts of the ROM only read state the core owns (get_flag, not_flag, get_character, item_count, is_tampered),
// so the core evaluates their bytecode itself (opcode semantics: re/buu/SCRIPT.md §3). Anything else -> None (the
// caller treats it as false and reports it).

fn varint(code: &[u8], pc: &mut usize) -> Option<i32> {
    let mut v: u32 = 0;
    loop {
        let b = *code.get(*pc)?;
        *pc += 1;
        v = (v << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    let v = v as i32;
    // zig-zag with arithmetic shifts (odd -> -(v >> 1))
    Some(if v & 1 != 0 { (v >> 1).wrapping_neg() } else { v >> 1 })
}

/// evaluate a condition script's bytecode on the story state (`current` = the leader's character): Some(stack[0] != 0)
pub fn eval_condition(code: &[u8], s: &Story, current: i32) -> Option<bool> {
    let mut st: Vec<i32> = Vec::with_capacity(16);
    let mut pc = 0usize;
    let mut steps = 0;
    while pc < code.len() {
        steps += 1;
        if steps > 10_000 || st.len() > 16 {
            return None;
        }
        let op = code[pc];
        pc += 1;
        match op {
            0x00 => {
                st.push(*code.get(pc)? as i32);
                pc += 1;
            }
            0x01 => st.push(varint(code, &mut pc)?),
            0x1D => {
                let n = *code.get(pc)?;
                pc += 1;
                for _ in 0..n {
                    st.push(varint(code, &mut pc)?);
                }
            }
            0x02 => {
                let n = *code.get(pc)?;
                pc += 1;
                match n {
                    0x1B => {
                        let f = st.pop()?;
                        st.push(s.flag(f) as i32)
                    }
                    0x1C => {
                        let f = st.pop()?;
                        st.push(!s.flag(f) as i32)
                    }
                    0x15 => st.push(current),
                    0x7F => {
                        let i = st.pop()?;
                        st.push(s.supply(i))
                    }
                    0xF6 => st.push(0),
                    _ => return None,
                }
            }
            0x04..=0x10 | 0x1E => {
                if op == 0x06 {
                    let a = st.pop()?;
                    st.push(a.wrapping_neg());
                    continue;
                }
                let b = st.pop()?;
                let a = st.pop()?;
                st.push(match op {
                    0x04 => (a != 0 && b != 0) as i32,
                    0x05 => (a != 0 || b != 0) as i32,
                    0x07 => a.wrapping_add(b),
                    0x08 => a.wrapping_sub(b),
                    0x09 => a.wrapping_mul(b),
                    0x0A => {
                        if b == 0 {
                            return None;
                        }
                        a.wrapping_div(b)
                    }
                    0x1E => {
                        if b == 0 {
                            return None;
                        }
                        a.wrapping_rem(b)
                    }
                    0x0B => (a == b) as i32,
                    0x0C => (a != b) as i32,
                    0x0D => (a > b) as i32,
                    0x0E => (a >= b) as i32,
                    0x0F => (a < b) as i32,
                    _ => (a <= b) as i32,
                });
            }
            0x11 => break,
            0x12 => {
                let o = pc;
                let d = u16::from_be_bytes([*code.get(pc)?, *code.get(pc + 1)?]) as usize;
                pc = o + d;
            }
            0x13 => {
                let o = pc;
                let d = u16::from_be_bytes([*code.get(pc)?, *code.get(pc + 1)?]) as usize;
                let v = st.pop()?;
                pc = if v == 0 { o + d } else { o + 2 };
            }
            0x14 => {
                st.pop()?;
            }
            _ => return None,
        }
    }
    Some(st.first().copied().unwrap_or(0) != 0)
}
