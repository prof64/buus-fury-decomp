//! The Battle page as a character select, drawn in the game's own menu style: the pause pages' striped background and
//! bevelled panels, the game's fonts and UI palette (`wf_core::menu::text::Canvas` -> `wf_render::menu::MenuRenderer`),
//! the Status page's portraits, the Skills page's skill icons, the game's A button.
//!
//! The stage: the chosen fighter's portrait with its moves (skill icons) and its idle animation at 2x facing right,
//! against the opponent's portrait (when the game has one) and idle animation facing left; names on top, the
//! opponent's level and how many under its name. Below: Form, Outfit (grey when the form has one look), Level,
//! Players, Arena and the fighter's numbers at that level (HP, EP, STR, POW, END, SPD as the battle has them).
//! UP / DOWN walk the rows (the launcher's row list), LEFT / RIGHT change the focused one - green between yellow
//! arrows - and A on any row starts the fight.

use std::collections::HashMap;
use std::sync::Arc;

use wf_content::sprite::Sheet;
use wf_content::Packs;
use wf_core::content::GameData;
use wf_core::menu::text::{units, Canvas, Pen, TextData};
use wf_render::menu::MenuRenderer;
use wf_view::MenuFont;

use super::{Launcher, Row};

/// UI palette indices (`ui/palette.json`)
const WHITE: u8 = 2;
const RED: u8 = 3;
const YELLOW: u8 = 5;
const GREEN: u8 = 6;
const GREY: u8 = 0x75;
const SHADOW: u8 = 1;

/// what the stage and the stat line show for the chosen fighter (built from a battle world once per choice)
#[derive(Clone, Debug, Default)]
struct FighterInfo {
    sprite: Option<i32>,
    hp: i32,
    ep: i32,
    /// STR, POW, END, SPD as the battle has them (form bonus and equipment included)
    stats: [i32; 4],
    skills: Vec<i32>,
    portrait: Option<String>,
}

#[derive(Default)]
pub struct PageState {
    menu: Option<MenuRenderer>,
    sheets: HashMap<String, Option<Arc<Sheet>>>,
    fighter: Option<(String, FighterInfo)>,
}

impl PageState {
    fn sheet(&mut self, packs: &Packs, key: &str) -> Option<Arc<Sheet>> {
        self.sheets.entry(key.to_string()).or_insert_with(|| Sheet::load(packs, key).ok().map(Arc::new)).clone()
    }
}

fn fighter_info(l: &Launcher, d: &Arc<GameData>) -> Option<FighterInfo> {
    let mut cfg = l.battle_config();
    cfg.players.truncate(1);
    cfg.phases.clear();
    let s = wf_core::session::Session::new(Arc::clone(d), cfg);
    let w = &s.world;
    let p = w.pl_ref(w.player_idx(0)?);
    let portrait = d.menu.as_ref().and_then(|m| {
        let pb = m.forms.get(p.form.max(0) as usize)?.portraits[0] as usize;
        m.portraits.get(pb.checked_sub(8)?).filter(|s| !s.is_empty()).cloned()
    });
    Some(FighterInfo {
        sprite: p.a.sprite,
        hp: p.rec.max_hp,
        ep: p.rec.max_ep,
        stats: [p.a.stats.str_, p.a.stats.pow, p.a.stats.end, p.eff.spd],
        skills: p.skills.clone(),
        portrait,
    })
}

/// a sprite's idle frame: (sheet, frame index, flip, halo attach point, shadow key + offset)
type Idle = (Arc<Sheet>, usize, bool, Option<(i32, i32)>, Option<(String, (i32, i32))>);

fn idle_frame(d: &GameData, st: &mut PageState, packs: &Packs, sprite: i32, dir: u8, t: u32) -> Option<Idle> {
    let sp = d.sprite(sprite)?;
    let sh = st.sheet(packs, &format!("sprites/{}", sp.id))?;
    let an = sh.def.anim("idle").or_else(|| sh.def.anim_by_raw(1).map(|a| a.1)).or_else(|| sh.def.animations.values().next())?;
    let (_, step) = an.step_at(dir, t, true)?;
    let shadow = sp.shadow.clone().filter(|s| s != "shadow_0x0").map(|s| (format!("effects/{s}"), sp.shadow_off));
    Some((Arc::clone(&sh), step.frame as usize, step.flip, step.attach.map(|a| (a[0], a[1])), shadow))
}

/// one sheet frame at `scale`, its anchor on (x, y), mirrored about the anchor when `flip`; clipped to `clip`
#[allow(clippy::too_many_arguments)]
fn blit(fb: &mut [u32], w: usize, h: usize, sheet: &Sheet, fi: usize, flip: bool, x: i32, y: i32, scale: i32, clip: [i32; 4]) {
    let Some(f) = sheet.def.frames.get(fi) else { return };
    let img = &sheet.image;
    for j in 0..f.h as i32 {
        for i in 0..f.w as i32 {
            let c = img.px[((f.y as i32 + j) as u32 * img.w + (f.x as i32 + i) as u32) as usize];
            if c >> 24 == 0 {
                continue;
            }
            let dx = if flip { f.anchor.x - 1 - i } else { i - f.anchor.x };
            let dy = j - f.anchor.y;
            for sy in 0..scale {
                for sx in 0..scale {
                    let (px, py) = (x + dx * scale + sx, y + dy * scale + sy);
                    if px < clip[0].max(0) || py < clip[1].max(0) || px >= clip[2].min(w as i32) || py >= clip[3].min(h as i32) {
                        continue;
                    }
                    fb[py as usize * w + px as usize] = 0xFF00_0000 | (c & 0x00FF_FFFF);
                }
            }
        }
    }
}

/// an actor's idle animation on the stage, feet on (x, y): shadow, body, halo (the field's OAM order, back to front),
/// at 2x when its frame fits `room` (else 1x)
#[allow(clippy::too_many_arguments)]
fn stage_actor(st: &mut PageState, packs: &Packs, d: &GameData, fb: &mut [u32], w: usize, h: usize, sprite: i32, dir: u8, t: u32, cx: i32, floor: i32, room: (i32, i32), clip: [i32; 4]) {
    // placed by the animation's first frame (its box centred on cx, its bottom on the floor): the entity position
    // then stays put while the frames change
    let Some((sh0, f0, flip0, _, _)) = idle_frame(d, st, packs, sprite, dir, 0) else { return };
    let Some(r) = sh0.def.frames.get(f0) else { return };
    let scale = if r.w as i32 * 2 <= room.0 && r.h as i32 * 2 <= room.1 { 2 } else { 1 };
    let (rw, rh, ax, ay) = (r.w as i32, r.h as i32, r.anchor.x, r.anchor.y);
    let x = if flip0 { cx - (ax * 2 - rw) * scale / 2 } else { cx - (rw - ax * 2) * scale / 2 };
    let y = floor - (rh - ay) * scale;
    let Some((sh, fi, flip, attach, shadow)) = idle_frame(d, st, packs, sprite, dir, t) else { return };
    if let Some((key, off)) = shadow {
        if let Some(s) = st.sheet(packs, &key) {
            blit(fb, w, h, &s, 0, false, x + off.0 * scale, y + off.1 * scale, scale, clip);
        }
    }
    blit(fb, w, h, &sh, fi, flip, x, y, scale, clip);
    if let (Some((hx, hy)), Some(halo)) = (attach, st.sheet(packs, "effects/halo")) {
        blit(fb, w, h, &halo, 0, false, x + hx * scale, y + hy * scale, scale, clip);
    }
}

/// a 3 x 5 arrow head pointing left (tip at x) / right (tip at x + 2), centred on row y
fn arrow(cv: &mut Canvas, x: i32, y: i32, left: bool) {
    for k in 0..3 {
        let c = if left { x + k } else { x + 2 - k };
        cv.fill(c, y - k, c + 1, y + k + 1, YELLOW);
    }
}

/// text, cut to `max_w` (".." when cut); focused: green between yellow arrows; `align` 0 left, 1 centre, 2 right
#[allow(clippy::too_many_arguments)]
fn item(cv: &mut Canvas, t: &TextData, font: MenuFont, s: &str, x: i32, y: i32, align: u8, colour: u8, focus: bool, max_w: i32) -> i32 {
    let fm = t.font(font);
    let mut u = units(s);
    if fm.measure(&u) > max_w {
        let dots = units("..");
        while !u.is_empty() && fm.measure(&u) + fm.measure(&dots) > max_w {
            u.pop();
        }
        u.extend(dots);
    }
    let wd = fm.measure(&u);
    let x0 = match align {
        1 => x - wd / 2,
        2 => x - wd,
        _ => x,
    };
    let mut p = Pen::new(font, x0, y, 0, if focus { GREEN } else { colour }, Some(SHADOW));
    cv.text(t, &mut p, &u);
    if focus {
        let mid = y - if font == MenuFont::Bold { 7 } else { 6 };
        arrow(cv, x0 - 6, mid, true);
        arrow(cv, x0 + wd + 2, mid, false);
    }
    wd
}

/// "Label  value": the label yellow at `x`, the value at `vx`
#[allow(clippy::too_many_arguments)]
fn line(cv: &mut Canvas, t: &TextData, label: &str, value: &str, x: i32, vx: i32, y: i32, colour: u8, focus: bool, max_w: i32) {
    let mut p = Pen::new(MenuFont::Menu, x, y, 0, YELLOW, Some(SHADOW));
    cv.text(t, &mut p, &units(label));
    // (room for the focus arrow between the label and the value)
    item(cv, t, MenuFont::Menu, value, vx.max(p.x + 9), y, 0, colour, focus, max_w);
}

/// Draw the Battle page (240 x 160 in the middle of `w` x `h`). False = the content has no menu data (the plain list
/// is drawn instead).
pub fn draw(l: &mut Launcher, packs: &Packs, fb: &mut [u32], w: usize, h: usize) -> bool {
    let Some(d) = l.data.clone() else { return false };
    let Some(md) = d.menu.clone() else { return false };
    let t = &md.text;
    let rows = l.rows();
    let focus = rows.get(l.cursor).copied();
    let on = |r: Row| focus == Some(r);
    let tick = l.tick;
    // the fighter's numbers at the chosen level (a battle world built once per choice)
    let key = serde_json::to_string(&l.battle_config().players.first()).unwrap_or_default();
    if l.page.fighter.as_ref().map(|f| &f.0) != Some(&key) {
        let info = fighter_info(l, &d).unwrap_or_default();
        l.page.fighter = Some((key, info));
    }
    let fi = l.page.fighter.as_ref().map(|f| f.1.clone()).unwrap_or_default();
    let pick = l.picks.get(l.pick_idx()).cloned();
    let b = l.settings.battle.clone();
    // the opponent on show, by the list's name
    let ety: i32 = b.enemy.trim_start_matches('t').parse().unwrap_or(-1);
    let enemy = d.enemy(ety);
    let opp = l.enemies.iter().find(|e| e.0 == b.enemy).map(|e| e.1.clone()).or_else(|| enemy.map(|e| e.name.clone())).unwrap_or_default();
    let elevel = l.enemy_levels.get(&ety).copied();

    let mut cv = Canvas::new();
    cv.background(0, 0, 240, 160);
    // the stage: names on top, under them the fighter's form and the opponent's level / how many; the portraits with
    // the actors in front of their inner edges; the fighter's moves down the left edge
    cv.panel(3, 1, 233, 93);
    for (x, p) in [(34, &fi.portrait)] {
        if let Some(p) = p {
            cv.fill(x - 1, 23, x + 65, 89, SHADOW);
            cv.image(p, 0, 64, 64, x, 24);
        }
    }
    for (k, &sk) in fi.skills.iter().take(5).enumerate() {
        if let Some(icon) = md.opts.skills.get(sk.max(0) as usize).map(|s| s.icon.clone()).filter(|s| !s.is_empty()) {
            cv.image(&icon, 0, 24, 11, 6, 24 + 13 * k as i32);
        }
    }
    let fname = pick.as_ref().map(|p| p.character.to_uppercase()).unwrap_or_default();
    let nw = item(&mut cv, t, MenuFont::Bold, &fname, 14, 17, 0, YELLOW, on(Row::Fighter), 96);
    // the level on the name's line, like the Status page's "GOKU LVL 60" (clear of the portrait's frame)
    let level = if b.level_auto { l.auto_level() } else { b.level.clamp(50, 200) };
    item(&mut cv, t, MenuFont::Small, &format!("LV {level}"), 14 + nw + if on(Row::Fighter) { 11 } else { 6 }, 17, 0, WHITE, false, 60);
    let oname = opp.to_uppercase();
    let focus_opp = on(Row::Enemy);
    // a long name (story fights, notes in brackets) on two lines in the menu font - no name is cut (playtest #10)
    let room = 104;
    let fits = |f: MenuFont, s: &str| t.font(f).measure(&units(s)) <= room;
    let mut sub_y = 27;
    if fits(MenuFont::Bold, &oname) {
        item(&mut cv, t, MenuFont::Bold, &oname, 224, 17, 2, YELLOW, focus_opp, room);
    } else if fits(MenuFont::Menu, &oname) {
        item(&mut cv, t, MenuFont::Menu, &oname, 224, 17, 2, YELLOW, focus_opp, room);
    } else {
        // the first kind of break (in this order) that leaves both lines within the room; the mark stays on line 1
        // ("JANEMBA >" / "SUPER JANEMBA"), a bracket moves to line 2
        let split = |sep: &str| {
            oname.match_indices(sep).map(|(i, _)| i).filter_map(|i| {
                let (a, b) = if sep == " (" { (&oname[..i], &oname[i + 1..]) } else { (&oname[..i + sep.trim_end().len()], &oname[i + sep.len()..]) };
                (fits(MenuFont::Menu, a) && t.font(MenuFont::Menu).measure(&units(b)) <= room + 6).then(|| (a.to_string(), b.to_string()))
            }).last()
        };
        let (a, b2) = [" (", " > ", " & ", ": ", ", ", " "].iter().find_map(|sep| split(sep)).unwrap_or((oname.clone(), String::new()));
        // (raised: the sub line stays above the opponent's sprite)
        item(&mut cv, t, MenuFont::Menu, &a, 224, 13, 2, YELLOW, focus_opp, room);
        item(&mut cv, t, MenuFont::Menu, &b2, 224, 22, 2, YELLOW, false, room + 6);
        sub_y = 31;
    }
    let osub = elevel.map(|lv| format!("LV {lv}")).unwrap_or_default();
    item(&mut cv, t, MenuFont::Small, &osub, 230, sub_y, 2, WHITE, false, 80);
    let mut vs = Pen::new(MenuFont::Bold, 158, 52, 1, RED, Some(SHADOW));
    cv.text(t, &mut vs, &units("VS"));
    // A to fight
    cv.image("ui/buttons/a.png", 0, 16, 16, 139, 70);
    let mut p = Pen::new(MenuFont::Bold, 156, 84, 0, YELLOW, Some(SHADOW));
    cv.text(t, &mut p, &units("GO"));

    // the setup
    cv.panel(3, 96, 233, 155);
    let (lx, vx) = (9, 44);
    if let Some(p) = &pick {
        line(&mut cv, t, "Form", &p.form, lx, vx, 109, WHITE, on(Row::Form), 176);
        let single = l.outfits().len() <= 1;
        line(&mut cv, t, "Outfit", &p.outfit, lx, vx, 119, if single { GREY } else { WHITE }, on(Row::Outfit), 176);
    }
    // (the default: an even match - the opponent's level)
    let level = if b.level_auto { format!("Even {}", l.auto_level()) } else { format!("{}", b.level.clamp(50, 200)) };
    line(&mut cv, t, "Level", &level, lx, vx, 129, WHITE, on(Row::Level), 60);
    line(&mut cv, t, "Players", &b.players.clamp(1, 4).to_string(), 170, 207, 129, WHITE, on(Row::Players), 20);
    let arena = l.label(Row::Arena).1.trim_start_matches("< ").trim_end_matches(" >").to_string();
    line(&mut cv, t, "Arena", &arena, lx, vx, 139, WHITE, on(Row::Arena), 176);
    // the fighter's numbers at that level (the Status page's colours: labels yellow, values white), spread over the
    // panel's width
    let stats = [("HP", fi.hp), ("EP", fi.ep), ("STR", fi.stats[0]), ("POW", fi.stats[1]), ("END", fi.stats[2]), ("SPD", fi.stats[3])];
    let sf = t.font(MenuFont::Small);
    // (inside the panel's border: x 9 .. 225; at high levels three-digit values left no room - "SPD 121" ran over the
    // border, playtest #18 - so the label / value space and the gaps shrink first)
    const ROOM: i32 = 216;
    let width = |sp: i32| -> Vec<i32> { stats.iter().map(|(n, v)| sf.measure(&units(n)) + sp + sf.measure(&units(&v.to_string()))).collect() };
    let sp = if width(3).iter().sum::<i32>() + 5 * 3 <= ROOM { 3 } else { 1 };
    let widths = width(sp);
    let gap = ((ROOM - widths.iter().sum::<i32>()) / 5).max(1);
    let mut x = lx;
    for ((n, v), wd) in stats.iter().zip(&widths) {
        let mut p = Pen::new(MenuFont::Small, x, 150, 0, YELLOW, Some(SHADOW));
        cv.text(t, &mut p, &units(n));
        let mut p = Pen::new(MenuFont::Small, p.x + sp, 150, 0, WHITE, Some(SHADOW));
        cv.text(t, &mut p, &units(&v.to_string()));
        x += wd + gap;
    }

    let mv = wf_core::menu::surface_view(cv);
    let mr = l.page.menu.get_or_insert_with(|| MenuRenderer::load(packs));
    let pic = mr.render(packs, &mv, None, 0);
    let (ox, oy) = ((w as i32 - 240) / 2, (h as i32 - 160) / 2);
    for yy in 0..160 {
        for xx in 0..240 {
            let (px, py) = (ox + xx, oy + yy);
            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                fb[py as usize * w + px as usize] = pic[(yy * 240 + xx) as usize];
            }
        }
    }
    // the actors on the stage, in front of the portraits: the fighter facing right, the opponent facing left
    let clip = [ox + 6, oy + 4, ox + 231, oy + 90];
    if let Some(s) = fi.sprite {
        stage_actor(&mut l.page, packs, &d, fb, w, h, s, 3, tick, ox + 122, oy + 89, (64, 68), clip);
    }
    if let Some(s) = enemy.and_then(|e| e.sprite) {
        stage_actor(&mut l.page, packs, &d, fb, w, h, s, 2, tick, ox + 200, oy + 89, (62, 68), clip);
    }
    true
}

/// One row of a list page: label, value ("< x >" = changeable with LEFT / RIGHT; empty = an action), highlighted
/// (the START row)
pub(super) struct ListRow {
    pub label: String,
    pub value: String,
    pub accent: bool,
}

/// The Story / Settings / Mods pages in the game's menu style, like the Battle page (playtest #21: they were a plain
/// dark text panel): the striped background, a title panel and a bevelled list panel, yellow labels, white values,
/// the focused row green between yellow arrows. False = the content has no menu data (the plain list is drawn).
#[allow(clippy::too_many_arguments)]
pub(super) fn list_page(l: &mut Launcher, packs: &Packs, fb: &mut [u32], w: usize, h: usize, title: &str, note: &str, rows: &[ListRow], cursor: usize, message: &str) -> bool {
    let Some(d) = l.data.clone() else { return false };
    let Some(md) = d.menu.clone() else { return false };
    let t = &md.text;
    let mut cv = Canvas::new();
    cv.background(0, 0, 240, 160);
    cv.panel(3, 1, 233, 21);
    item(&mut cv, t, MenuFont::Bold, &title.to_uppercase(), 12, 16, 0, YELLOW, false, 120);
    if !note.is_empty() {
        item(&mut cv, t, MenuFont::Small, note, 228, 15, 2, WHITE, false, 150);
    }
    cv.panel(3, 23, 233, 145);
    // the value column after the widest label; long lists scroll
    let fm = t.font(MenuFont::Menu);
    let lw = rows.iter().filter(|r| !r.value.is_empty()).map(|r| fm.measure(&units(&r.label))).max().unwrap_or(0);
    let vx = (14 + lw + 12).max(74);
    let room = (226 - vx).max(40);
    let visible = 10usize;
    let first = cursor.saturating_sub(visible - 1);
    for (k, r) in rows.iter().enumerate().skip(first).take(visible) {
        let y = 38 + (k - first) as i32 * 11;
        let focus = k == cursor;
        let changeable = r.value.starts_with("< ") && r.value.ends_with(" >");
        let label_col = if r.accent { RED } else { YELLOW };
        if r.value.is_empty() {
            // an action: the label is the focus
            item(&mut cv, t, MenuFont::Menu, &r.label, 14, y, 0, label_col, focus, 208);
            continue;
        }
        let mut p = Pen::new(MenuFont::Menu, 14, y, 0, if focus && !changeable { GREEN } else { label_col }, Some(SHADOW));
        cv.text(t, &mut p, &units(&r.label));
        let v = if changeable { &r.value[2..r.value.len() - 2] } else { r.value.as_str() };
        let grey = v.starts_with("- ") || v == "-";
        item(&mut cv, t, MenuFont::Menu, v, vx, y, 0, if grey { GREY } else { WHITE }, focus && changeable, room);
    }
    let hint = if message.is_empty() { "A OK   LEFT / RIGHT CHANGE   B BACK" } else { message };
    item(&mut cv, t, MenuFont::Small, hint, 12, 157, 0, if message.is_empty() { GREY } else { RED }, false, 216);
    let mv = wf_core::menu::surface_view(cv);
    let mr = l.page.menu.get_or_insert_with(|| MenuRenderer::load(packs));
    let pic = mr.render(packs, &mv, None, 0);
    let (ox, oy) = ((w as i32 - 240) / 2, (h as i32 - 160) / 2);
    for yy in 0..160 {
        for xx in 0..240 {
            let (px, py) = (ox + xx, oy + yy);
            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                fb[py as usize * w + px as usize] = pic[(yy * 240 + xx) as usize];
            }
        }
    }
    true
}
