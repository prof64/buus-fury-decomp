//! Draw rules of the field objects (read-only on the World): coins, dropped items, map items, map props /
//! breakables, level signs, debris chips, the item-get icon. Content: `effects/coin_0..3`, `field_items` (frame =
//! supply index, 100 + equipment index), `props_<map>` (frame = prop index), `field_labels`, `debris`, the shadows.
//! Rules and verification: re/buu/notes.md "Field: object graphics", engine/docs/world.md §6.

use crate::fieldobj::{MapSprite, Pickup, PickupKind};
use crate::tables::SIN256;
use crate::world::{Body, World};
use wf_view::{FrameSel, RenderView, SpriteDraw, SpriteEffect};

fn put(out: &mut RenderView, entity: u32, sheet: String, index: i32, x: i32, y: i32, priority: u8, effect: SpriteEffect) {
    out.sprites.push(SpriteDraw { entity, sheet, frame: FrameSel::Frame { index: index.max(0) as u16, flip: false }, x, y, priority, mirror: false, effect });
}

fn item_frame(item: i32) -> i32 {
    if item >= 0 {
        item
    } else {
        100 - item
    }
}

/// coins (`0x08018284`) + shadow (`0x08018614`), dropped items (+ the player's 10x4 shadow), map items
pub fn pickup(w: &World, id: u32, p: &Pickup, out: &mut RenderView) {
    match p.kind {
        PickupKind::Zenie(s) => {
            // hidden on frames with fc & 4 once older than 480 frames
            if p.t > 0x1E0 && w.frame & 4 != 0 {
                return;
            }
            let mut cel = (((p.phase as i32).wrapping_add(p.t)) & 0xFF) >> 3;
            if cel > 3 {
                cel = 0;
            }
            put(out, id, format!("effects/coin_{}", s.clamp(0, 3)), cel, p.x >> 8, (p.y >> 8) - (p.z >> 8), 3, SpriteEffect::None);
            let sh = if s & 1 == 1 { "shadow_10x4" } else { "shadow_4x2" };
            put(out, id, format!("effects/{sh}"), 0, p.bx[0], p.bx[1], 3, SpriteEffect::SemiTransparent);
        }
        PickupKind::Supply(i) | PickupKind::Equipment(i) => {
            let item = if matches!(p.kind, PickupKind::Supply(_)) { i } else { -i };
            put(out, id, "effects/field_items".into(), item_frame(item), p.bx[0], p.bx[1] - (p.z >> 10), 3, SpriteEffect::None);
            put(out, id, "effects/shadow_10x4".into(), 0, p.bx[0], p.bx[1] + 10, 3, SpriteEffect::SemiTransparent);
        }
        PickupKind::MapItem { item, .. } => put(out, id, "effects/field_items".into(), item_frame(item), p.bx[0], p.bx[1], 3, SpriteEffect::None),
    }
}

fn prop_visible(w: &World, k: i32) -> bool {
    w.field.as_ref().and_then(|f| f.props.get(k as usize)).is_some_and(|p| p.visible)
}

/// level signs (`0x0800C378`): the label at (x, y + (sine[fc & 0xFF] >> 14)) while its prop is visible
pub fn map_sprite(w: &World, id: u32, m: &MapSprite, out: &mut RenderView) {
    let Some((frame, prop)) = m.label else { return };
    if prop >= 0 && !prop_visible(w, prop) {
        return;
    }
    let bob = SIN256[(w.frame & 0xFF) as usize] >> 14;
    put(out, id, "effects/field_labels".into(), frame, m.x, m.y + bob, 3, SpriteEffect::None);
}

/// map props (`0x0800CF7A`, visible ones, at the rect top-left; a shaking breakable adds its offset)
pub fn props(w: &World, out: &mut RenderView) {
    let Some(f) = w.field.as_ref() else { return };
    let Some(md) = w.data.field.as_ref().and_then(|d| d.maps.get(&f.map)) else { return };
    for (k, p) in f.props.iter().enumerate() {
        if !p.visible {
            continue;
        }
        let Some(def) = md.props.get(k) else { continue };
        let Some(frame) = def.frame else { continue };
        let (mut dx, mut dy) = (0, 0);
        for e in &w.ents {
            if let Body::Breakable(b) = &e.body {
                if b.prop == k as i32 && !e.removed {
                    dx = b.dx;
                    dy = b.dy;
                }
            }
        }
        put(out, 0, format!("effects/props_{}", f.map), frame, p.rect.x0 + dx, p.rect.y0 + dy, def.priority, SpriteEffect::None);
    }
}

/// the debris chips and the item-get icon (priority 2, rising 1 px per 16 frames above the player)
pub fn front(w: &World, out: &mut RenderView) {
    let Some(f) = w.field.as_ref() else { return };
    if let Some(g) = &f.item_get {
        if let (Some(pi), Some(item)) = (w.player_idx(g.slot as usize), g.item) {
            let a = &w.pl_ref(pi).a;
            let k = (60 - g.left).max(0);
            put(out, 0, "effects/field_items".into(), item_frame(item), a.px(), a.py() - 40 - ((k + 2) >> 4), 2, SpriteEffect::None);
        }
    }
    for d in &f.debris {
        put(out, 0, "effects/debris".into(), 0, d.x >> 8, d.y >> 8, 3, SpriteEffect::None);
    }
}
