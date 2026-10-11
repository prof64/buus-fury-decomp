//! Draw rules of the field objects (read-only on the World): coins, dropped items, map items, map props /
//! breakables, level signs, debris chips, the item-get icon. Content: `effects/coin_0..3`, `field_items` (frame =
//! supply index, 100 + equipment index), `props_<map>` (frame = prop index), `field_labels`, `debris`, the shadows.
//! Rules and verification: re/buu/notes.md "Field: object graphics", engine/docs/world.md §6.

use crate::fieldobj::{MapSprite, Pickup, PickupKind};
use crate::tables::SIN256;
use crate::world::{Body, World};
use wf_view::{FrameSel, RenderView, SpriteDraw, SpriteEffect};

fn put(
    out: &mut RenderView,
    entity: u32,
    sheet: String,
    index: i32,
    x: i32,
    y: i32,
    priority: u8,
    effect: SpriteEffect,
) {
    out.sprites.push(SpriteDraw {
        entity,
        sheet,
        frame: FrameSel::Frame {
            index: index.max(0) as u16,
            flip: false,
        },
        x,
        y,
        priority,
        mirror: false,
        effect,
    });
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
            if p.t > 0x1E0 && w.g.clock(w.frame) & 4 != 0 {
                return;
            }
            let mut cel = (((p.phase as i32).wrapping_add(p.t + p.spin)) & 0xFF) >> 3;
            if cel > 3 {
                cel = 0;
            }
            put(
                out,
                id,
                format!("effects/coin_{}", s.clamp(0, 3)),
                cel,
                p.x >> 8,
                (p.y >> 8) - (p.z >> 8),
                3,
                SpriteEffect::None,
            );
            let sh = if s & 1 == 1 {
                "shadow_10x4"
            } else {
                "shadow_4x2"
            };
            put(
                out,
                id,
                format!("effects/{sh}"),
                0,
                p.bx[0],
                p.bx[1],
                3,
                SpriteEffect::SemiTransparent,
            );
        }
        PickupKind::Supply(i) | PickupKind::Equipment(i) => {
            let item = if matches!(p.kind, PickupKind::Supply(_)) {
                i
            } else {
                -i
            };
            put(
                out,
                id,
                "effects/field_items".into(),
                item_frame(item),
                p.bx[0],
                p.bx[1] - (p.z >> 10),
                3,
                SpriteEffect::None,
            );
            put(
                out,
                id,
                "effects/shadow_10x4".into(),
                0,
                p.bx[0],
                p.bx[1] + 10,
                3,
                SpriteEffect::SemiTransparent,
            );
        }
        PickupKind::MapItem { item, .. } => put(
            out,
            id,
            "effects/field_items".into(),
            item_frame(item),
            p.bx[0],
            p.bx[1] - (p.z >> 10),
            3,
            SpriteEffect::None,
        ),
    }
}

fn prop_visible(w: &World, k: i32) -> bool {
    w.field
        .as_ref()
        .and_then(|f| f.props.get(k as usize))
        .is_some_and(|p| p.visible)
}

/// level signs (`0x0800C378`): the label at (x, y + (sine[fc & 0xFF] >> 14)) while its prop is visible
pub fn map_sprite(w: &World, id: u32, m: &MapSprite, out: &mut RenderView) {
    let Some((frame, prop)) = m.label else { return };
    if prop >= 0 && !prop_visible(w, prop) {
        return;
    }
    // (the frame counter 0x03000F94: `Globals::clock`, not the world's own frame - tour_earth_east_a f1130)
    let bob = SIN256[(w.g.clock(w.frame) & 0xFF) as usize] >> 14;
    put(
        out,
        id,
        "effects/field_labels".into(),
        frame,
        m.x,
        m.y + bob,
        3,
        SpriteEffect::None,
    );
}

/// map props (`0x0800CF7A`, visible ones, at the rect top-left; a shaking breakable adds its offset)
pub fn props(w: &World, out: &mut RenderView) {
    for (k, _, _) in prop_keys(w) {
        prop(w, k, out);
    }
}

/// the visible props with their draw sort key: each prop is a world-list object of its own (vtable 0x0804D1E4)
/// sorted with the actors by (layer +8, depth +4; 0x08052EE0: a higher layer is further back, then a larger depth
/// further front) - its layer the record's byte +0x16 (the tile word's bits 16..23: 0x38 like the actors, 0x3C
/// behind them), its depth the rect's bottom (+0x30); an actor's depth is its y + 4. Emulator: Earth 7-2's drinks
/// stand, the counter props (layer 0x38, depth 704) over the girl at 700; Airship 12-17's lift (0x3C) behind the
/// player. Returned as (prop, our layer = 0x38 - layer, depth on the actors' y scale = bottom - 4)
pub fn prop_keys(w: &World) -> Vec<(usize, i32, i32)> {
    let Some((md, live)) = map_props(w) else {
        return vec![];
    };
    md.props
        .iter()
        .enumerate()
        .filter_map(|(k, d)| {
            let (visible, rect) = match live {
                Some(f) => {
                    let p = f.props.get(k)?;
                    (p.visible, p.rect)
                }
                None => (d.visible, d.rect),
            };
            (visible && d.frame.is_some()).then_some((k, 0x38 - ((d.tile >> 16) & 0xFF) as i32, rect.y1 - 4))
        })
        .collect()
}

/// The props of the map on show: the field's (with their live state), else a battle arena's map (`w.arena.map`
/// "8-70" -> `area_08_070`), whose props stand as the map loads them - the game draws the arena maps' props (Rocky
/// Canyon 8-70's four rock pillars, emulator; their bases are solid in the map's own walk grid either way).
fn map_props(w: &World) -> Option<(&crate::fieldcontent::MapDef, Option<&crate::field::Field>)> {
    let fd = w.data.field.as_ref()?;
    match w.field.as_ref() {
        Some(f) => fd.maps.get(&f.map).map(|m| (m, Some(&**f))),
        None => fd.maps.get(&battle_map_id(&w.arena.map)?).map(|m| (m, None)),
    }
}

/// "8-70" -> "area_08_070"
fn battle_map_id(map: &str) -> Option<String> {
    let (a, b) = map.split_once('-')?;
    Some(format!("area_{:02}_{:03}", a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))
}

/// a prop's rect (the map's, or the field's live one)
pub fn prop_rect(w: &World, k: usize) -> Option<crate::geom::Rect> {
    let (md, live) = map_props(w)?;
    match live {
        Some(f) => f.props.get(k).map(|p| p.rect),
        None => md.props.get(k).map(|d| d.rect),
    }
}

/// one prop (and its breakable's shake)
pub fn prop(w: &World, k: usize, out: &mut RenderView) {
    let Some((md, live)) = map_props(w) else { return };
    let Some(def) = md.props.get(k) else { return };
    let rect = match live {
        Some(f) => match f.props.get(k) {
            Some(p) => p.rect,
            None => return,
        },
        None => def.rect,
    };
    let Some(frame) = def.frame else { return };
    // the prop's draw check (0x0800CF42): its rect against the camera's rect without the screen shake (0x030019A4
    // +0x18; the OAM is placed with the shaken position +0x28) - a prop just off the screen is not drawn though the
    // shake's offset brings its edge onto it (emulator, HFIL 18-10 f163 .. f170: the right column's props, the
    // rect's x1 582 against their 584, under the punch's (3, 3) shake)
    if w.field.is_some()
        && !w.cameras.is_empty()
        && w.cameras.iter().all(|c| rect.x1 <= c.rect.x0 || rect.x0 >= c.rect.x1 || rect.y1 <= c.rect.y0 || rect.y0 >= c.rect.y1)
    {
        return;
    }
    let (mut dx, mut dy) = (0, 0);
    for e in &w.ents {
        if let Body::Breakable(b) = &e.body {
            if b.prop == k as i32 && !e.removed {
                dx = b.dx;
                dy = b.dy;
            }
        }
    }
    put(
        out,
        0,
        format!("effects/props_{}", md.id),
        frame,
        rect.x0 + dx,
        rect.y0 + dy,
        def.priority,
        SpriteEffect::None,
    );
}

/// the debris chips and the item-get icon (priority 2, rising 1 px per 16 frames above the player)
pub fn front(w: &World, out: &mut RenderView) {
    if let Some(g) = w.link_get.as_ref().filter(|g| g.left > 0) {
        if let (Some(pi), Some(item)) = (w.player_idx(g.slot as usize), g.supply) {
            let a = &w.pl_ref(pi).a;
            let k = (60 - g.left).max(0);
            put(out, 0, "effects/field_items".into(), item_frame(item), a.px(), a.py() - 40 - ((k + 2) >> 4), 2, SpriteEffect::None);
        }
    }
    let Some(f) = w.field.as_ref() else { return };
    if let Some(g) = &f.item_get {
        if let (Some(pi), Some(item)) = (w.player_idx(g.slot as usize), g.item) {
            let a = &w.pl_ref(pi).a;
            let k = (60 - g.left).max(0);
            put(
                out,
                0,
                "effects/field_items".into(),
                item_frame(item),
                a.px(),
                a.py() - 40 - ((k + 2) >> 4),
                2,
                SpriteEffect::None,
            );
        }
    }
}

/// the first draw-order index of the debris chips (`debris_keys`; the props' are below it)
pub const DEBRIS_BASE: usize = 1 << 20;

/// a breakable's debris chips in the draw order: world-list objects (0x0800C454: layer 0x38, their depth set by
/// the draw check 0x0800C4DA to their y) sorted with the actors and props - behind the rock they chip off while it
/// stands (emulator, HFIL 18-10 f163 ..: depths 64 .. 76 against the boulder's 80). Returned as (chip, depth on the
/// actors' y scale)
pub fn debris_keys(w: &World) -> Vec<(usize, i32)> {
    let Some(f) = w.field.as_ref() else { return vec![] };
    f.debris.iter().enumerate().map(|(k, d)| (k, (d.y >> 8) - 4)).collect()
}

pub fn debris(w: &World, k: usize, out: &mut RenderView) {
    if let Some(d) = w.field.as_ref().and_then(|f| f.debris.get(k)) {
        put(out, 0, "effects/debris".into(), 0, d.x >> 8, d.y >> 8, 3, SpriteEffect::None);
    }
}
