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
    /// the 3D world map's HUD object
    Map3d,
    /// a HUD object sliding out (`hud_out`)
    HudOut,
    /// a damage number (`popups`)
    Popup,
    /// an ended effect object's block (`delayed`)
    Ending,
    /// the boss HP bar (`show_boss_bar` 0x8F / 0x90, vtable 0x0804CE28: 0x200)
    BossBar,
    /// a menu screen's character figure
    Figure(u32),
    /// a cutscene ki blast (`fire_blast`, class 0x08027CBC: 0x100) of the shooting actor
    Blast(crate::world::EntityId),
    /// a keyed shared graphic (`keyed`)
    Keyed(u32),
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
    pub shared_rc: [u8; 5],
    pub suspended: Vec<crate::world::EntityId>,
    /// HUD objects sliding out after `hide_hud` (handle, updates left; the object deletes itself at the end)
    #[serde(default)]
    pub hud_out: Vec<(u16, i32)>,
    /// damage-number objects (`0x0800AD3E`, vtable 0x0804C904, 0x100 tiles): (target, frame of the target's last
    /// new number, handle, last frame it shows)
    #[serde(default)]
    pub popups: Vec<(crate::world::EntityId, u32, u16, u32)>,
    /// other refcounted shared graphics (`0x0804E490` records): (key, users)
    #[serde(default)]
    pub keyed: Vec<(u32, u32)>,
    /// the object world's allocator while a menu screen's own object world has taken its place (`world_hide`):
    /// allocator, owners, shared counts
    #[serde(default)]
    pub stashed: Option<Box<(ObjVram, Vec<(u16, Owner)>, [u8; 5])>>,
    /// blocks of effect objects that ended (the object deletes itself in its next update): (handle, frame)
    #[serde(default)]
    pub delayed: Vec<(u16, u32)>,
    /// the non-actor objects with tiles a purge can take (HUD, banner, boss bar): owner, the entity id the next
    /// object got when it was made (its place in the world list), its tile bytes
    #[serde(default)]
    pub objs: Vec<(Owner, u32, i32)>,
    /// effects whose block outlives the entity by a frame (`linger`)
    #[serde(default)]
    pub late: Vec<crate::world::EntityId>,
    /// actors whose shadow a script set (`shadow_set`): the shared graphic's index, None = no shadow
    #[serde(default)]
    pub shadow_set: Vec<(crate::world::EntityId, Option<u8>)>,
    /// what a purge suspended, in order (`suspended` = its entities)
    #[serde(default)]
    pub susp: Vec<Susp>,
}

/// an object a purge suspended
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Susp {
    Ent(crate::world::EntityId),
    Own(Owner, u32, i32),
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
    let stamp = w.next_id;
    if let (Some(h), Some(v)) = (h, st(w)) {
        v.owners.push((h, owner));
        if matches!(owner, Owner::Hud | Owner::Banner | Owner::BossBar) && !v.objs.iter().any(|o| o.0 == owner) {
            v.objs.push((owner, stamp, bytes));
        }
    }
}

/// free every block of an owner (allocation order)
pub fn release(w: &mut World, owner: Owner) {
    let Some(v) = st(w) else { return };
    let hs: Vec<u16> = v.owners.iter().filter(|o| o.1 == owner).map(|o| o.0).collect();
    v.owners.retain(|o| o.1 != owner);
    v.objs.retain(|o| o.0 != owner);
    v.susp.retain(|x| !matches!(x, Susp::Own(o, _, _) if *o == owner));
    for h in hs {
        v.a.free(h);
    }
}

/// the shared graphics an actor's sprite uses: its halo (def +0x50: `0x03001BA8`, 0x80 bytes) and its shadow
/// (`0x086B759C` by def +0xA / +0xB)
pub type Shares = (bool, Option<usize>);
/// index of the halo's shared graphic in `shared_rc`
pub const HALO: usize = 4;

fn shares_of(sp: &crate::content::Sprite) -> Shares {
    (sp.halo, SHADOWS.iter().position(|s| (s.0, s.1) == sp.shadow_key))
}
fn shared_bytes(k: usize) -> i32 {
    if k == HALO {
        0x80
    } else {
        SHADOWS[k].2
    }
}

/// `0x0804E490`: the first user of a shared graphic allocates it
pub fn shared_acquire(w: &mut World, k: usize) {
    let Some(v) = st(w) else { return };
    v.shared_rc[k] = v.shared_rc[k].wrapping_add(1);
    if v.shared_rc[k] == 1 {
        acquire(w, Owner::Shared(k as u8), shared_bytes(k));
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

fn sprite_of(w: &World, i: usize) -> Option<(i32, Shares)> {
    let s = w.actor(i)?.sprite?;
    let sp = w.data.sprite(s)?;
    let mut sh = shares_of(sp);
    let id = w.ents[i].id;
    if let Some(o) = w.field.as_ref().and_then(|f| f.vram.as_ref()).and_then(|v| v.shadow_set.iter().find(|o| o.0 == id)) {
        sh.1 = o.1.map(|k| k as usize);
    }
    Some((sp.tile_bytes, sh))
}

/// `set_attachment` (`0x0800ECB0`): the actor's shadow record becomes the one of (w, h) in `0x086B759C` (none: no
/// shadow) - the old shared graphic's reference goes, the new one's is taken (segment db7 14-1 -> 0-9: s019's
/// detach frees the shadow graphic)
pub fn shadow_set(w: &mut World, i: usize, key: (i32, i32)) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    let old = sprite_of(w, i).and_then(|s| s.1 .1);
    let new = SHADOWS.iter().position(|s| (s.0 as i32, s.1 as i32) == key);
    let live = st(w).is_some_and(|v| !v.suspended.contains(&id) && v.owners.iter().any(|o| o.1 == Owner::Ent(id)));
    if live {
        if let Some(k) = old {
            shared_release(w, k);
        }
    }
    if let Some(v) = st(w) {
        v.shadow_set.retain(|o| o.0 != id);
        v.shadow_set.push((id, new.map(|k| k as u8)));
    }
    if live {
        if let Some(k) = new {
            shared_acquire(w, k);
        }
    }
}

fn map_3d(w: &World) -> bool {
    w.field.as_ref().is_some_and(|f| w.data.field.as_ref().and_then(|d| d.maps.get(&f.map)).is_some_and(|m| m.three_d))
}

/// tiles, halo, shadow (the ctor `0x0800EB60` and the re-acquire `0x0800F4E6`)
fn take(w: &mut World, id: EntityId, bytes: i32, sh: Shares) {
    acquire(w, Owner::Ent(id), bytes);
    if sh.0 {
        shared_acquire(w, HALO);
    }
    if let Some(k) = sh.1 {
        shared_acquire(w, k);
    }
}

/// an actor's sprite instance (`0x0800EB60`): its tiles (def +2), its halo's and its shadow's shared graphics
pub fn actor_created(w: &mut World, i: usize) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    let Some((bytes, sh)) = sprite_of(w, i) else { return };
    take(w, id, bytes, sh);
}

/// an actor's instance destroyed (`0x0800EC34`): its shadow's reference, its halo's, then its tiles
pub fn actor_destroyed(w: &mut World, id: EntityId, sh: Shares) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let owned = st(w).is_some_and(|v| v.owners.iter().any(|o| o.1 == Owner::Ent(id)));
    let was_suspended = st(w).is_some_and(|v| v.suspended.contains(&id));
    if let Some(v) = st(w) {
        v.suspended.retain(|&s| s != id);
        v.susp.retain(|x| *x != Susp::Ent(id));
    }
    if !owned || was_suspended {
        return;
    }
    if let Some(k) = sh.1 {
        shared_release(w, k);
    }
    if sh.0 {
        shared_release(w, HALO);
    }
    release(w, Owner::Ent(id));
}

/// frames the boss bar object lives after `hide_boss_bar` (it slides out, then deletes itself: Kid Buu's 21-26)
pub const BOSS_BAR_OUT: u32 = 17;

/// an owner's blocks go `frames` object updates from now (an object that ends by itself)
pub fn release_later(w: &mut World, owner: Owner, frames: u32) {
    let at = w.frame + frames;
    let Some(v) = st(w) else { return };
    let hs: Vec<u16> = v.owners.iter().filter(|o| o.1 == owner).map(|o| o.0).collect();
    for h in hs {
        v.owners.retain(|o| o.0 != h);
        v.owners.push((h, Owner::Ending));
        v.delayed.push((h, at));
    }
}

/// the blocks of entity `i` go (its object is destroyed)
pub fn entity_gone(w: &mut World, i: usize) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    // an animated effect's block goes with our entity (segments sb 21-26: a script's puffs; sb 19-3: Kid Buu's
    // teleport puffs, his beam's explosions), except a broken object's explosion, whose block lives one frame
    // longer (`linger`; sb 21-5). MEASURED: the cause of that extra frame is not decoded (the effect's update
    // `0x0802A8AA` is the same). Other effect kinds keep the extra frame (not measured apart).
    let late = st(w).is_some_and(|v| v.late.contains(&id));
    if let Some(v) = st(w) {
        v.late.retain(|&x| x != id);
    }
    let exact = match &w.ents[i].body {
        Body::Fx(f) => late || !matches!(**f, crate::fx::Fx::Anim { .. }),
        _ => false,
    };
    if exact && w.ents[i].removed {
        let at = w.frame + 1;
        if let Some(v) = st(w) {
            let hs: Vec<u16> = v.owners.iter().filter(|o| o.1 == Owner::Ent(id)).map(|o| o.0).collect();
            for h in hs {
                v.owners.retain(|o| o.0 != h);
                v.owners.push((h, Owner::Ending));
                v.delayed.push((h, at));
            }
        }
        return;
    }
    let sh = sprite_of(w, i).map(|s| s.1).unwrap_or((false, None));
    actor_destroyed(w, id, sh);
}

/// effect `id`'s block will live one frame past the entity's end (see `entity_gone`)
pub fn linger(w: &mut World, id: EntityId) {
    if let Some(v) = st(w) {
        v.late.push(id);
    }
}

/// the offstage players' blocks go (a scene load destroys their objects)
pub fn offstage_gone(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let off: Vec<(EntityId, Shares)> = w
        .scene
        .offstage
        .iter()
        .map(|e| {
            let sh = match &e.body {
                Body::Player(p) => p.a.sprite.and_then(|s| w.data.sprite(s)).map(shares_of).unwrap_or((false, None)),
                _ => (false, None),
            };
            (e.id, sh)
        })
        .collect();
    for (id, sh) in off {
        actor_destroyed(w, id, sh);
    }
}

/// the removed entities' blocks go (before the list drops them), in the update's order: the list from its end
/// (segment sb 20-8: two effects ending in one frame, the later object's block first)
pub fn reap(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    for i in (0..w.ents.len()).rev() {
        if w.ents[i].removed {
            entity_gone(w, i);
        }
    }
}

/// SetSprite (`0x0800EDF8`) of actor `i`, whose sprite was `old`: the old def's halo reference goes; a different
/// tile size frees the block and allocates the new size (`0x0801183E`, purging if needed); the new def's halo
/// reference comes. The shadow stays the old one's.
pub fn sprite_changed(w: &mut World, i: usize, old: Option<i32>) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let id = w.ents[i].id;
    if !st(w).is_some_and(|v| v.owners.iter().any(|o| o.1 == Owner::Ent(id))) {
        return;
    }
    let (ob, oh) = old.and_then(|s| w.data.sprite(s)).map(|sp| (sp.tile_bytes, sp.halo)).unwrap_or((0, false));
    let Some((bytes, sh)) = sprite_of(w, i) else { return };
    if oh {
        shared_release(w, HALO);
    }
    if ob != bytes {
        release(w, Owner::Ent(id));
        acquire(w, Owner::Ent(id), bytes);
    }
    if sh.0 {
        shared_acquire(w, HALO);
    }
}

/// what a purge does with an object at a level (vfunc +0x18)
#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    Keep,
    /// 1: the object is destroyed
    Destroy,
    /// 2: suspended (out of the list until the text box closes)
    Suspend,
}

/// an entity's answer: the player from level 15 (`0x08011050`); an actor from 14 on screen / 13 off screen
/// (`0x0800F4C0`; 15 when it is the camera's object `0x03001BAC`); an animated effect is destroyed from level 2
/// (`0x0802A9EC`), a projectile from 3 (`0x08019D00`); other objects are suspended from 15 (`0x0801155C`)
fn ent_act(w: &World, i: usize, level: i32) -> Act {
    let at = |n: i32, a: Act| if level >= n { a } else { Act::Keep };
    match &w.ents[i].body {
        Body::Player(_) => at(15, Act::Suspend),
        Body::Npc(_) | Body::Enemy(_) => at(if on_screen(w, i) { 14 } else { 13 }, Act::Suspend),
        Body::Fx(f) if matches!(**f, crate::fx::Fx::Anim { .. } | crate::fx::Fx::Loop { .. }) => at(2, Act::Destroy),
        Body::Shot(_) => at(3, Act::Destroy),
        _ => Act::Keep,
    }
}
/// a non-actor object's: the banner is suspended from level 2 (`0x0800C060`), the HUD and the boss bar from 4
/// (`0x0800BD86` / `0x0801978C`)
fn own_act(o: Owner, level: i32) -> Act {
    let n = match o {
        Owner::Banner => 2,
        Owner::Hud | Owner::BossBar => 4,
        _ => return Act::Keep,
    };
    if level >= n {
        Act::Suspend
    } else {
        Act::Keep
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

/// `0x08011732`: until the largest free block holds `need`, go through the world list from its end, level by level
/// (0, 1, ...), asking every object (vfunc +0x18): destroyed or suspended ones give their tiles; from level 11 the
/// room is checked after each one. A damage number is destroyed at any level (`0x0800AD10`).
pub fn purge(w: &mut World, need: i32) {
    let objects = |w: &World| w.ents.iter().filter(|e| !e.removed && !suspended(w, e.id)).count();
    let room = |w: &World| w.field.as_ref().and_then(|f| f.vram.as_ref()).map(|v| v.a.room(objects(w))).unwrap_or(i32::MAX);
    if room(w) >= need {
        return;
    }
    #[derive(Clone, Copy)]
    enum Item {
        Ent(EntityId),
        Own(Owner, u32, i32),
        Popup(u16),
    }
    let mut level = 0;
    while room(w) < need && level < 64 {
        // the list now: the entities in order, each non-actor object before the first entity made after it; the
        // damage numbers by their handles' entities (after their targets: made later)
        let mut items: Vec<Item> = vec![];
        {
            let Some(v) = w.field.as_ref().and_then(|f| f.vram.as_ref()) else { return };
            let mut owns: Vec<(Owner, u32, i32)> = v.objs.iter().filter(|o| !v.susp.iter().any(|x| matches!(x, Susp::Own(a, _, _) if *a == o.0))).copied().collect();
            owns.sort_by_key(|o| o.1);
            let mut k = 0;
            for e in w.ents.iter().filter(|e| !e.removed && !v.suspended.contains(&e.id)) {
                while k < owns.len() && owns[k].1 <= e.id {
                    items.push(Item::Own(owns[k].0, owns[k].1, owns[k].2));
                    k += 1;
                }
                items.push(Item::Ent(e.id));
            }
            for o in &owns[k..] {
                items.push(Item::Own(o.0, o.1, o.2));
            }
            for p in &v.popups {
                items.push(Item::Popup(p.2));
            }
        }
        for it in items.into_iter().rev() {
            let acted = match it {
                Item::Popup(h) => {
                    if let Some(v) = st(w) {
                        v.popups.retain(|p| p.2 != h);
                        v.owners.retain(|o| o.0 != h);
                        v.a.free(h);
                    }
                    true
                }
                Item::Own(o, stamp, bytes) => {
                    if own_act(o, level) == Act::Suspend {
                        release(w, o);
                        if let Some(v) = st(w) {
                            v.susp.push(Susp::Own(o, stamp, bytes));
                        }
                        true
                    } else {
                        false
                    }
                }
                Item::Ent(id) => {
                    let Some(i) = w.find(id) else { continue };
                    match ent_act(w, i, level) {
                        Act::Keep => false,
                        Act::Suspend => {
                            suspend(w, i);
                            true
                        }
                        Act::Destroy => {
                            release(w, Owner::Ent(id));
                            w.ents[i].removed = true;
                            true
                        }
                    }
                }
            };
            if acted && level >= 11 && room(w) >= need {
                break;
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
    // halo, shadow, tiles
    let sh = sprite_of(w, i).map(|s| s.1).unwrap_or((false, None));
    if sh.0 {
        shared_release(w, HALO);
    }
    if let Some(k) = sh.1 {
        shared_release(w, k);
    }
    release(w, Owner::Ent(id));
    if let Some(v) = st(w) {
        v.suspended.push(id);
        v.susp.push(Susp::Ent(id));
    }
}

/// `0x08011E94`: the suspended objects come back (last suspended first), appended to the world list, each taking
/// its tiles again (vfunc +0x20: an actor `0x0800F4E6` tiles, halo, shadow; purging if needed)
pub fn restore(w: &mut World) {
    let Some(v) = st(w) else { return };
    let list = std::mem::take(&mut v.susp);
    v.suspended.clear();
    for it in list.into_iter().rev() {
        match it {
            Susp::Ent(id) => {
                let Some(i) = w.find(id) else { continue };
                let e = w.ents.remove(i);
                w.ents.push(e);
                let i = w.ents.len() - 1;
                let Some((bytes, sh)) = sprite_of(w, i) else { continue };
                take(w, id, bytes, sh);
            }
            Susp::Own(o, _, bytes) => acquire(w, o, bytes),
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
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    // the 3D world map's own object (`0x0800C0C8`, vtable 0x0804D8C8, 0xF00), whatever `hide_hud` said
    if map_3d(w) {
        acquire(w, Owner::Map3d, 0xF00);
        return;
    }
    if w.scene.hud_hidden {
        return;
    }
    acquire(w, Owner::Hud, 0x500);
}

/// every scene visual's object goes (an area load)
pub fn visuals_gone(w: &mut World) {
    let Some(v) = st(w) else { return };
    let hs: Vec<u16> = v.owners.iter().filter(|o| matches!(o.1, Owner::Visual(_) | Owner::Blast(_))).map(|o| o.0).collect();
    v.owners.retain(|o| !matches!(o.1, Owner::Visual(_) | Owner::Blast(_)));
    for h in hs {
        v.a.free(h);
    }
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

/// a menu screen takes the object world's place (message 0xA to every object, `0x0800F45C`): the actors give
/// their halo's and shadow's shared graphics back (their tiles stay); the message goes through the list from its
/// end, like the update (the leader first)
pub fn world_hide(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    for i in (0..w.ents.len()).rev() {
        if w.ents[i].removed || suspended(w, w.ents[i].id) {
            continue;
        }
        let Some((_, sh)) = sprite_of(w, i) else { continue };
        if sh.0 {
            shared_release(w, HALO);
        }
        if let Some(k) = sh.1 {
            shared_release(w, k);
        }
    }
    // the screen's object world has its own (fresh) allocator; the field's is kept as it is
    if let Some(v) = st(w) {
        let a = std::mem::replace(&mut v.a, ObjVram::fresh());
        let o = std::mem::take(&mut v.owners);
        let rc = std::mem::replace(&mut v.shared_rc, [0; 5]);
        v.stashed = Some(Box::new((a, o, rc)));
    }
}
/// the object world is back (message 0xB, `0x0800F478`): its allocator, then halo, shadow again
pub fn world_show(w: &mut World) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    if let Some(v) = st(w) {
        if let Some(s) = v.stashed.take() {
            let (a, o, rc) = *s;
            v.a = a;
            v.owners = o;
            v.shared_rc = rc;
        }
    }
    for i in (0..w.ents.len()).rev() {
        if w.ents[i].removed || suspended(w, w.ents[i].id) {
            continue;
        }
        let Some((_, sh)) = sprite_of(w, i) else { continue };
        if sh.0 {
            shared_acquire(w, HALO);
        }
        if let Some(k) = sh.1 {
            shared_acquire(w, k);
        }
    }
}
/// a menu screen's character figure `k` (an actor object of sprite `sprite`): tiles, halo, shadow
pub fn figure_created(w: &mut World, k: u32, sprite: i32) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    let Some((bytes, sh)) = w.data.sprite(sprite).map(|sp| (sp.tile_bytes, shares_of(sp))) else { return };
    acquire(w, Owner::Figure(k), bytes);
    if sh.0 {
        shared_acquire(w, HALO);
    }
    if let Some(s) = sh.1 {
        shared_acquire(w, s);
    }
}
/// the figures go with their screen, last made first (each: shadow, halo, tiles)
pub fn figures_gone(w: &mut World, sprites: &[i32]) {
    if w.field.as_ref().is_none_or(|f| f.vram.is_none()) {
        return;
    }
    for (k, &sprite) in sprites.iter().enumerate().rev() {
        let Some(sh) = w.data.sprite(sprite).map(shares_of) else { continue };
        if let Some(s) = sh.1 {
            shared_release(w, s);
        }
        if sh.0 {
            shared_release(w, HALO);
        }
        release(w, Owner::Figure(k as u32));
    }
}

/// key of the breakables' debris chip graphic (4 chips per hit share it, 0x40 bytes; `0x0800C454`)
pub const KEY_DEBRIS: u32 = 1;

/// a user of a keyed shared graphic appears: the first one allocates it
pub fn keyed_acquire(w: &mut World, key: u32, bytes: i32) {
    let Some(v) = st(w) else { return };
    match v.keyed.iter_mut().find(|k| k.0 == key) {
        Some(k) => k.1 += 1,
        None => {
            v.keyed.push((key, 1));
            acquire(w, Owner::Keyed(key), bytes);
        }
    }
}
/// ... and goes: the last one frees it
pub fn keyed_release(w: &mut World, key: u32) {
    let Some(v) = st(w) else { return };
    let Some(k) = v.keyed.iter_mut().find(|k| k.0 == key) else { return };
    k.1 -= 1;
    if k.1 == 0 {
        v.keyed.retain(|k| k.0 != key);
        release(w, Owner::Keyed(key));
    }
}

/// a damage number over an actor (`0x0800E8E6`): within 8 frames of the actor's last new number the number object
/// is reused (its 16 frames start again); else a new object, when 0x100 bytes are free (`0x0800AD3E`)
pub fn damage_number(w: &mut World, target: EntityId) {
    let now = w.frame;
    let objects = w.ents.iter().filter(|e| !e.removed).count();
    let Some(v) = st(w) else { return };
    if let Some(p) = v.popups.iter_mut().find(|p| p.0 == target && now.wrapping_sub(p.1) < 8) {
        p.3 = now + POPUP_LIFE;
        return;
    }
    v.popups.retain(|p| p.0 != target || p.3 >= now);
    if v.a.room(objects) < 0x100 {
        return;
    }
    if let Some(h) = v.a.alloc(0x100) {
        v.owners.push((h, Owner::Popup));
        v.popups.push((target, now, h, now + POPUP_LIFE));
    }
}
/// frames a damage number lives after its (re)start
pub const POPUP_LIFE: u32 = 16;

/// the HUD objects' update (after the actors): a sliding-out one goes at the end of its slide; the damage numbers
/// end
pub fn hud_tick(w: &mut World) {
    let now = w.frame;
    let Some(v) = st(w) else { return };
    let ended: Vec<u16> = v.delayed.iter().filter(|d| d.1 <= now).map(|d| d.0).collect();
    if !ended.is_empty() {
        v.delayed.retain(|d| d.1 > now);
        // (the list updates from its end: the later object's block goes first)
        for h in ended.into_iter().rev() {
            v.owners.retain(|o| o.0 != h);
            v.a.free(h);
        }
    }
    let gone: Vec<u16> = v.popups.iter().filter(|p| p.3 <= now).map(|p| p.2).collect();
    if !gone.is_empty() {
        // the entry stays for the reuse window's frame stamp only while the object lives
        v.popups.retain(|p| p.3 > now);
        for h in gone {
            v.owners.retain(|o| o.0 != h);
            v.a.free(h);
        }
    }
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
    if std::env::var("DBG_VRAM").is_ok() {
        eprintln!("  banner {on} frame {}", w.frame);
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
            v.shared_rc = [0; 5];
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
