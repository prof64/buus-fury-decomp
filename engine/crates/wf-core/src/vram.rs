//! The object tile allocator (OBJ VRAM) of the field's world (allocator struct = the world `0x03001C4C`: free list
//! head +0, spare-entry list head +1, 138 block entries of 10 bytes at +0x694, the start -> entry index table at
//! +0xC00). 512 units of 64 bytes; `alloc` (`0x0801124C`) is best fit with an exact match taken at once, a larger
//! block is split and the allocation takes its top end; `free` (`0x0801142A`) merges with free neighbours in address
//! order. An area load resets it (`fresh`). Every sprite-showing object holds one block (actors: their sprite def's
//! tile bytes, def +2); a text box takes 0x1000 / 0x1000 / (portrait 0x1000) / 0x800. When an allocation does not
//! fit, `0x08011732` suspends objects (see `field::vram_purge`). VERIFIED: tools/superbuu/vramsim.py replays every
//! alloc / free of the Super Buu route (190,593 frames) against the emulator's allocator bytes.

use serde::{Deserialize, Serialize};

pub const ENTRIES: usize = 138;
pub const UNITS: u16 = 512;
const FF: u8 = 0xFF;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block {
    /// 0 free, 1 used, 0xFF spare
    pub used: u8,
    pub start: u16,
    pub size: u16,
    /// address-order neighbours (higher / lower); spare entries: the spare list (next / previous)
    pub hi: u8,
    pub lo: u8,
    /// the free list (next / previous)
    pub fnext: u8,
    pub fprev: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjVram {
    pub free_head: u8,
    pub spare_head: u8,
    pub e: Vec<Block>,
    /// start unit -> entry (`+0xC00`, never cleared by a free)
    pub idx: Vec<u8>,
}

impl Default for ObjVram {
    fn default() -> Self {
        Self::fresh()
    }
}

impl ObjVram {
    /// the state an area load leaves: entry 0 = the whole free block, 1.. spare in order
    pub fn fresh() -> ObjVram {
        let mut e = vec![Block::default(); ENTRIES];
        e[0] = Block { used: 0, start: 0, size: UNITS, hi: FF, lo: FF, fnext: FF, fprev: FF };
        for (i, b) in e.iter_mut().enumerate().skip(1) {
            *b = Block {
                used: FF,
                start: 0xFFFF,
                size: 0xFFFF,
                hi: if i + 1 < ENTRIES { (i + 1) as u8 } else { FF },
                lo: if i > 1 { (i - 1) as u8 } else { FF },
                fnext: FF,
                fprev: FF,
            };
        }
        ObjVram { free_head: 0, spare_head: 1, e, idx: vec![0; UNITS as usize] }
    }

    /// `0x080116FC`: the largest free block in bytes (0 when the object list is full, 0x80 objects)
    pub fn room(&self, objects: usize) -> i32 {
        if objects == 0x80 {
            return 0;
        }
        let mut m = 0;
        let mut i = self.free_head;
        while i != FF {
            m = m.max(self.e[i as usize].size as i32);
            i = self.e[i as usize].fnext;
        }
        m << 6
    }

    fn unlink(&mut self, i: u8) {
        let (fnext, fprev) = (self.e[i as usize].fnext, self.e[i as usize].fprev);
        if fprev == FF {
            self.free_head = fnext;
        } else {
            self.e[fprev as usize].fnext = fnext;
        }
        if fnext != FF {
            self.e[fnext as usize].fprev = fprev;
        }
    }
    fn push_free(&mut self, i: u8) {
        let h = self.free_head;
        let b = &mut self.e[i as usize];
        b.used = 0;
        b.fprev = FF;
        b.fnext = h;
        if h != FF {
            self.e[h as usize].fprev = i;
        }
        self.free_head = i;
    }
    fn spare(&mut self, i: u8) {
        let h = self.spare_head;
        let b = &mut self.e[i as usize];
        b.used = FF;
        b.lo = FF;
        b.hi = h;
        if h != FF {
            self.e[h as usize].lo = i;
        }
        self.spare_head = i;
    }
    /// `0x080113B0`: i absorbs its higher neighbour j (j's place in the free list), j becomes spare
    fn merge(&mut self, i: u8, j: u8) {
        let bj = self.e[j as usize];
        let bi = &mut self.e[i as usize];
        bi.size = bi.size.wrapping_add(bj.size);
        bi.hi = bj.hi;
        if bj.hi != FF {
            self.e[bj.hi as usize].lo = i;
        }
        self.e[i as usize].fprev = bj.fprev;
        if bj.fprev != FF {
            self.e[bj.fprev as usize].fnext = i;
        } else {
            self.free_head = i;
        }
        self.e[i as usize].fnext = bj.fnext;
        if bj.fnext != FF {
            self.e[bj.fnext as usize].fprev = i;
        }
        self.spare(j);
    }

    /// `0x0801124C`: a block of `bytes` (>> 6 units); the handle (start unit * 2), or None
    pub fn alloc(&mut self, bytes: i32) -> Option<u16> {
        let r = self.alloc_inner(bytes);
        if std::env::var("DBG_VRAM").is_ok() {
            eprintln!("  alloc {bytes:#x} -> {r:x?}");
        }
        r
    }
    fn alloc_inner(&mut self, bytes: i32) -> Option<u16> {
        let need = (bytes >> 6) as u16;
        let (mut best, mut bsz) = (FF, u32::MAX);
        let mut i = self.free_head;
        while i != FF {
            let s = self.e[i as usize].size;
            if (s as u32) < bsz {
                if s == need {
                    self.e[i as usize].used = 1;
                    self.unlink(i);
                    let st = self.e[i as usize].start;
                    self.idx[st as usize & 511] = i;
                    return Some(st << 1);
                }
                if s > need {
                    best = i;
                    bsz = s as u32;
                }
            }
            i = self.e[i as usize].fnext;
        }
        if best == FF {
            return None;
        }
        if self.spare_head == FF {
            self.e[best as usize].used = 1;
            self.unlink(best);
            let st = self.e[best as usize].start;
            self.idx[st as usize & 511] = best;
            return Some(st << 1);
        }
        let s = self.spare_head;
        self.spare_head = self.e[s as usize].hi;
        if self.spare_head != FF {
            self.e[self.spare_head as usize].lo = FF;
        }
        let bh = self.e[best as usize].hi;
        self.e[s as usize].lo = best;
        self.e[s as usize].hi = bh;
        if bh != FF {
            self.e[bh as usize].lo = s;
        }
        self.e[best as usize].hi = s;
        self.e[best as usize].size -= need;
        let st = self.e[best as usize].start + self.e[best as usize].size;
        let b = &mut self.e[s as usize];
        b.used = 1;
        b.start = st;
        b.size = need;
        self.idx[st as usize & 511] = s;
        Some(st << 1)
    }

    /// `0x0801142A`: free the block of a handle
    pub fn free(&mut self, handle: u16) {
        if std::env::var("DBG_VRAM").is_ok() {
            eprintln!("  free {handle:#x}");
        }
        let i = self.idx[(handle >> 1) as usize & 511];
        let b = self.e[i as usize];
        self.e[i as usize].used = 0;
        let (hi, lo) = (b.hi, b.lo);
        if hi == FF {
            if lo == FF {
                self.free_head = i;
                self.e[i as usize].fnext = FF;
                self.e[i as usize].fprev = FF;
                return;
            }
            if self.e[lo as usize].used == 0 {
                self.e[lo as usize].size = self.e[lo as usize].size.wrapping_add(b.size);
                self.e[lo as usize].hi = FF;
                self.spare(i);
            } else {
                self.push_free(i);
            }
            return;
        }
        let hu = self.e[hi as usize].used;
        if lo == FF {
            if hu == 0 {
                self.merge(i, hi);
            } else {
                self.push_free(i);
            }
            return;
        }
        let lu = self.e[lo as usize].used;
        if hu != 0 {
            if lu == 0 {
                self.e[lo as usize].size = self.e[lo as usize].size.wrapping_add(b.size);
                self.e[lo as usize].hi = b.hi;
                self.e[hi as usize].lo = b.lo;
                self.spare(i);
            } else {
                self.push_free(i);
            }
            return;
        }
        if lu != 0 {
            self.merge(i, hi);
            return;
        }
        self.unlink(hi);
        let h = self.e[hi as usize];
        self.e[lo as usize].size = b.size.wrapping_add(h.size).wrapping_add(self.e[lo as usize].size);
        self.e[lo as usize].hi = h.hi;
        if h.hi != FF {
            self.e[h.hi as usize].lo = b.lo;
        }
        self.spare(hi);
        self.spare(i);
    }

    /// the comparable state (a spare entry's start / size are stale)
    pub fn key(&self) -> Vec<(u8, u16, u16, u8, u8, u8, u8)> {
        let mut v = vec![(self.free_head, 0, 0, self.spare_head, 0, 0, 0)];
        for b in &self.e {
            v.push(if b.used == FF { (FF, 0, 0, b.hi, b.lo, 0, 0) } else { (b.used, b.start, b.size, b.hi, b.lo, b.fnext, b.fprev) });
        }
        v
    }
}

/// who holds a block
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Owner {
    /// an object's own sprite tiles (obj +0xC)
    Ent(crate::world::EntityId),
    /// a refcounted shared graphic: 0..3 the shadows of `0x086B759C`
    Shared(u8),
    /// the text box's blocks (`0x08012030`)
    Text,
    /// the text box's portrait block
    TextPortrait,
    /// a title text object (`show_title_text` 0xB2, vtable 0x0804B3D8: ((width + 0x21) >> 5 << 5) * 32 bytes)
    Title,
    /// a scene visual's object (an emote bubble: `emote` 0x5A / 0x5B, vtable 0x0804A298, 0x40), by its `vid`
    Visual(u32),
    /// a choice box's cursor (vtable 0x0804C974, `0x080178C8`: 0x100)
    Cursor,
    /// the map's props object (vtable 0x0804D1E4, `0x0800D15A`: the props' graphics size)
    Props,
    /// the HUD object (vtable 0x08049FEC, ctor `0x0800B854`: 0x500)
    Hud,
    /// a HUD object sliding out (`hud_out`)
    HudOut,
    /// the area-name banner (vtable 0x0804B528, ctor `0x0800BF34`: 0xF00)
    Banner,
    /// an import's block of an object the remake does not model
    Foreign,
}

/// the shared shadow graphics (`0x086B759C`: w, h, bytes)
pub const SHADOWS: [(u8, u8, i32); 4] = [(4, 2, 0x40), (10, 4, 0x40), (16, 6, 0x40), (24, 10, 0x100)];

/// the field's object tile state: the allocator, its blocks' owners (allocation order), the shared graphics'
/// reference counts, the objects a purge suspended (`world +0x40C`, in suspension order)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VramState {
    pub a: ObjVram,
    pub owners: Vec<(u16, Owner)>,
    pub shared_rc: [u8; 4],
    pub suspended: Vec<crate::world::EntityId>,
    /// HUD objects sliding out after `hide_hud` (handle, updates left; the object deletes itself at the end)
    #[serde(default)]
    pub hud_out: Vec<(u16, i32)>,
}

use crate::world::{Body, EntityId, World};

fn st(w: &mut World) -> Option<&mut VramState> {
    w.field.as_mut().and_then(|f| f.vram.as_mut())
}

/// an object is out of the world list (a purge suspended it)
pub fn suspended(w: &World, id: EntityId) -> bool {
    w.field.as_ref().and_then(|f| f.vram.as_ref()).is_some_and(|v| v.suspended.contains(&id))
}

/// `0x08011818`-style: alloc, else purge for it and alloc again
pub fn acquire(w: &mut World, owner: Owner, bytes: i32) {
    if bytes <= 0 {
        return;
    }
    let Some(v) = st(w) else { return };
    let h = match v.a.alloc(bytes) {
        Some(h) => Some(h),
        None => {
            purge(w, bytes);
            st(w).and_then(|v| v.a.alloc(bytes))
        }
    };
    if let (Some(h), Some(v)) = (h, st(w)) {
        v.owners.push((h, owner));
    }
}

/// free every block of an owner (allocation order)
pub fn release(w: &mut World, owner: Owner) {
    let Some(v) = st(w) else { return };
    let hs: Vec<u16> = v.owners.iter().filter(|o| o.1 == owner).map(|o| o.0).collect();
    v.owners.retain(|o| o.1 != owner);
    for h in hs {
        v.a.free(h);
    }
}

fn shadow_index(key: (u8, u8)) -> Option<usize> {
    SHADOWS.iter().position(|s| (s.0, s.1) == key)
}

/// `0x0804E490`: the first user of a shared graphic allocates it
pub fn shared_acquire(w: &mut World, k: usize) {
    let Some(v) = st(w) else { return };
    v.shared_rc[k] = v.shared_rc[k].wrapping_add(1);
    if v.shared_rc[k] == 1 {
        acquire(w, Owner::Shared(k as u8), SHADOWS[k].2);
    }
}
/// `0x0804FE20`: the last user frees it
pub fn shared_release(w: &mut World, k: usize) {
    let Some(v) = st(w) else { return };
    if v.shared_rc[k] == 0 {
        return;
    }
    v.shared_rc[k] -= 1;
    if v.shared_rc[k] == 0 {
        release(w, Owner::Shared(k as u8));
    }
}

fn sprite_of(w: &World, i: usize) -> Option<(i32, (u8, u8))> {
    let s = w.actor(i)?.sprite?;
    let sp = w.data.sprite(s)?;
    Some((sp.tile_bytes, sp.shadow_key))
}

/// an actor's sprite instance (`0x0800EB60`): its tiles (def +2), then its shadow's shared graphic
pub fn actor_created(w: &mut World, i: usize) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    let Some((bytes, key)) = sprite_of(w, i) else { return };
    acquire(w, Owner::Ent(id), bytes);
    if let Some(k) = shadow_index(key) {
        shared_acquire(w, k);
    }
}

/// an actor's instance destroyed (`0x0800EC34`): its tiles, then its shadow's reference
pub fn actor_destroyed(w: &mut World, id: EntityId, key: (u8, u8)) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let owned = st(w).is_some_and(|v| v.owners.iter().any(|o| o.1 == Owner::Ent(id)));
    let was_suspended = st(w).is_some_and(|v| v.suspended.contains(&id));
    if let Some(v) = st(w) {
        v.suspended.retain(|&s| s != id);
    }
    if !owned || was_suspended {
        return;
    }
    // the shadow's reference first, then the own block (load frees: 21-5 shared, actor, actor, ...)
    if let Some(k) = shadow_index(key) {
        shared_release(w, k);
    }
    release(w, Owner::Ent(id));
}

/// the blocks of entity `i` go (its object is destroyed)
pub fn entity_gone(w: &mut World, i: usize) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    let key = sprite_of(w, i).map(|s| s.1).unwrap_or((0, 0));
    actor_destroyed(w, id, key);
}

/// the offstage players' blocks go (a scene load destroys their objects)
pub fn offstage_gone(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let off: Vec<(EntityId, (u8, u8))> = w
        .scene
        .offstage
        .iter()
        .map(|e| {
            let key = match &e.body {
                Body::Player(p) => p.a.sprite.and_then(|s| w.data.sprite(s)).map(|s| s.shadow_key).unwrap_or((0, 0)),
                _ => (0, 0),
            };
            (e.id, key)
        })
        .collect();
    for (id, key) in off {
        actor_destroyed(w, id, key);
    }
}

/// the removed entities' blocks go, in list order (before the list drops them)
pub fn reap(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    for i in 0..w.ents.len() {
        if w.ents[i].removed {
            entity_gone(w, i);
        }
    }
}

/// a sprite change of an actor (`0x0801183E`): the old block freed, the new size allocated (purging if needed)
pub fn actor_resized(w: &mut World, i: usize) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    let Some((bytes, _)) = sprite_of(w, i) else { return };
    release(w, Owner::Ent(id));
    acquire(w, Owner::Ent(id), bytes);
}

/// the purge level at which an object is suspended (vfunc +0x18; 2 = suspend): the player 15 (`0x08011050`); an
/// actor 15 when it is the camera's object (`0x03001BAC`), else 14 on screen / 13 off screen (`0x0800F4C0`);
/// other objects 15 (`0x0801155C`)
fn purge_level(w: &World, i: usize) -> i32 {
    match &w.ents[i].body {
        Body::Player(_) => 15,
        Body::Npc(_) | Body::Enemy(_) => {
            if on_screen(w, i) {
                14
            } else {
                13
            }
        }
        _ => 15,
    }
}

/// vfunc +0xC (`0x0801003A`): the actor's current cel overlaps the camera rect
pub fn on_screen(w: &World, i: usize) -> bool {
    let Some(a) = w.actor(i) else { return false };
    let r = &w.cameras[0].rect;
    let (x, y) = (a.px(), a.py());
    let b = crate::scene::cel_box(w, i).unwrap_or([0, 0, 16, 16]);
    let (x0, y0, x1, y1) = (x + b[0], y + b[1], x + b[0] + b[2], y + b[1] + b[3]);
    x0 < r.x1 && r.x0 < x1 && y0 < r.y1 && r.y0 < y1
}

/// `0x08011732`: until the largest free block holds `need`, suspend objects from the end of the world list, level
/// by level (from level 11 the room is checked after each one)
pub fn purge(w: &mut World, need: i32) {
    let objects = |w: &World| w.ents.iter().filter(|e| !e.removed && !suspended(w, e.id)).count();
    let room = |w: &World| w.field.as_ref().and_then(|f| f.vram.as_ref()).map(|v| v.a.room(objects(w))).unwrap_or(i32::MAX);
    if room(w) >= need {
        return;
    }
    let mut level = 0;
    while room(w) < need && level < 64 {
        let mut k = w.ents.len();
        while k > 0 {
            k -= 1;
            if w.ents[k].removed || suspended(w, w.ents[k].id) {
                continue;
            }
            if level >= purge_level(w, k) {
                suspend(w, k);
                if level >= 11 && room(w) >= need {
                    break;
                }
            }
        }
        level += 1;
    }
}

/// vfunc +0x1C (`0x0800F516`): the object leaves the world list, its tiles and shadow reference go
fn suspend(w: &mut World, i: usize) {
    let id = w.ents[i].id;
    if std::env::var("DBG_VRAM").is_ok() {
        eprintln!("SUSPEND f{} ent {} idx {} sprite {:?} onscreen {}", w.frame, id, i, crate::scene::sprite_id(w, i), on_screen(w, i));
    }
    let key = sprite_of(w, i).map(|s| s.1);
    if let Some(k) = key.and_then(shadow_index) {
        shared_release(w, k);
    }
    release(w, Owner::Ent(id));
    if let Some(v) = st(w) {
        v.suspended.push(id);
    }
}

/// `0x08011E94`: the suspended objects come back (last suspended first), appended to the world list, each taking
/// its tiles and shadow again (vfunc +0x20 `0x0800F4E6`, purging if needed)
pub fn restore(w: &mut World) {
    let Some(v) = st(w) else { return };
    let list = std::mem::take(&mut v.suspended);
    for &id in list.iter().rev() {
        let Some(i) = w.find(id) else { continue };
        let e = w.ents.remove(i);
        w.ents.push(e);
        let i = w.ents.len() - 1;
        let Some((bytes, key)) = sprite_of(w, i) else { continue };
        acquire(w, Owner::Ent(id), bytes);
        if let Some(k) = shadow_index(key) {
            shared_acquire(w, k);
        }
    }
}

/// a text box opens (`0x08012030`): 0x1000, 0x1000, a portrait 0x1000 (box kinds other than 0 / 7), 0x800; each
/// purges first (`0x08011732`)
pub fn text_open(w: &mut World, portrait: bool) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let mut sizes = vec![0x1000, 0x1000];
    if portrait {
        sizes.push(0x1000);
    }
    sizes.push(0x800);
    let n_sizes = sizes.len();
    for (k, n) in sizes.into_iter().enumerate() {
        purge(w, n);
        if let Some(v) = st(w) {
            if let Some(h) = v.a.alloc(n) {
                v.owners.push((h, if portrait && k == n_sizes - 2 { Owner::TextPortrait } else { Owner::Text }));
            }
        }
    }
}

/// the text box closes: its blocks freed (the portrait's first, then the others last allocated first), the
/// suspended objects restored (`0x08012180`; segment sb 4-24)
pub fn text_close(w: &mut World) {
    let Some(v) = st(w) else { return };
    let mut hs: Vec<u16> = v.owners.iter().filter(|o| o.1 == Owner::TextPortrait).map(|o| o.0).collect();
    hs.extend(v.owners.iter().filter(|o| o.1 == Owner::Text).map(|o| o.0).rev());
    if hs.is_empty() {
        return;
    }
    v.owners.retain(|o| o.1 != Owner::Text && o.1 != Owner::TextPortrait);
    for h in hs {
        v.a.free(h);
    }
    restore(w);
}

/// the HUD object of an arrival (after the players; none while the HUD is hidden)
pub fn hud_created(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) || w.scene.hud_hidden {
        return;
    }
    acquire(w, Owner::Hud, 0x500);
}

/// HUD frames of the slide-out after `hide_hud` before the object deletes itself (VERIFIED 4-21: 25)
pub const HUD_OUT: i32 = 25;

/// `show_hud` (`0x080046F4`): a new HUD object when the HUD was hidden; `hide_hud` (`0x08004764`): the object slides
/// out and goes `HUD_OUT` updates later
pub fn hud_shown(w: &mut World, on: bool) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    if on {
        acquire(w, Owner::Hud, 0x500);
    } else if let Some(v) = st(w) {
        let hs: Vec<u16> = v.owners.iter().filter(|o| o.1 == Owner::Hud).map(|o| o.0).collect();
        for h in hs {
            v.owners.retain(|o| o.0 != h);
            v.owners.push((h, Owner::HudOut));
            v.hud_out.push((h, HUD_OUT));
        }
    }
}

/// the HUD objects' update (after the actors): a sliding-out one goes at the end of its slide
pub fn hud_tick(w: &mut World) {
    let Some(v) = st(w) else { return };
    if v.hud_out.is_empty() {
        return;
    }
    for o in v.hud_out.iter_mut() {
        o.1 -= 1;
    }
    let gone: Vec<u16> = v.hud_out.iter().filter(|o| o.1 <= 0).map(|o| o.0).collect();
    v.hud_out.retain(|o| o.1 > 0);
    for h in gone {
        v.owners.retain(|o| o.0 != h);
        v.a.free(h);
    }
}
/// the area banner starts / ends
pub fn banner(w: &mut World, on: bool) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    release(w, Owner::Banner);
    if on {
        acquire(w, Owner::Banner, 0xF00);
    }
}

/// `show_title_text` (`0x0802BFBC`): its string's width rounded up to 32 px (+1) * 32 bytes, purging first
pub fn title_shown(w: &mut World, string: i32) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let width = w.data.field.as_ref().and_then(|f| f.title_widths.get(string as usize).copied()).unwrap_or(0);
    let bytes = ((width + 0x21) >> 5 << 5) << 5;
    purge(w, bytes);
    if let Some(v) = st(w) {
        if let Some(h) = v.a.alloc(bytes) {
            v.owners.push((h, Owner::Title));
        }
    }
}

/// an area load after the old objects went: nothing left allocated -> the allocator is re-initialised
pub fn load_reset(w: &mut World) {
    if let Some(v) = st(w) {
        if v.owners.is_empty() {
            v.a = ObjVram::fresh();
            v.shared_rc = [0; 4];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alloc_free_round_trip() {
        let mut v = ObjVram::fresh();
        let a = v.alloc(1024).unwrap();
        let b = v.alloc(512).unwrap();
        assert_eq!(a, (512 - 16) * 2);
        assert_eq!(b, (512 - 24) * 2);
        v.free(a);
        v.free(b);
        assert_eq!(v.room(1), 512 << 6);
        assert_eq!(v.free_head, 0);
    }
}
