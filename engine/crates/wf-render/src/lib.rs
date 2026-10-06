//! Software compositor with GBA semantics: turns a [`RenderView`] + content into an ARGB framebuffer of any size.
//!
//! Why software: the game is 2D tiles + OBJs at integer positions; composing them on the CPU the way the GBA PPU
//! does (BG priorities, OBJ-over-BG ties, top-two-layer alpha blending with BLDCNT/EVA/EVB, tile animations,
//! per-layer scroll rates) is cheap (~1-3 ms for a 640x360 zoomed-out view) and is pixel-exact by construction —
//! identical on every platform, testable with golden hashes. The host only uploads the framebuffer and scales
//! it by an integer factor (canvas `drawImage` with smoothing off on the web, a nearest-neighbour blit natively).
//!
//! Composition order (GBA): for priority 3 down to 0, the BGs with that priority (BG3 first, BG0 last), then the
//! OBJs with that priority (last OAM entry first). The two top-most opaque pixels are kept per screen pixel for
//! blending.

pub mod banner;
pub mod font;
pub mod menu;
pub mod mode7;
pub mod mode7_draw;
pub mod ui;

use std::collections::HashMap;
use std::sync::Arc;

use wf_content::map::{LayerKind, Map};
use wf_content::sprite::Sheet;
use wf_content::Packs;
use wf_view::{FrameSel, RenderView, SpriteEffect, GBA_H, GBA_W};

/// How the window maps to world pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// The original 240x160 screen, scaled by the largest integer that fits (or `scale` when > 0).
    Gba { scale: u32 },
    /// PC "zoomed out": the view is the window size / an integer zoom (0 = auto: the smallest zoom >= 2 whose
    /// view the map covers with <= 8 % black margin, max 6 - picked when the map changes, [`AutoZoom`]), centred
    /// on the game's camera.
    Pc { zoom: u32 },
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Pc { zoom: 0 }
    }
}

/// View size in world pixels and the integer scale to present it at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub view_w: u32,
    pub view_h: u32,
    pub scale: u32,
}

pub fn auto_zoom(win_w: u32, win_h: u32, world: Option<(i32, i32)>) -> u32 {
    let mut z = 2;
    if let Some((ww, wh)) = world.filter(|(w, h)| *w > 0 && *h > 0) {
        let black = |z: u32| {
            let vw = (win_w / z).max(1) as f64;
            let vh = (win_h / z).max(1) as f64;
            1.0 - (ww as f64 / vw).min(1.0) * (wh as f64 / vh).min(1.0)
        };
        while z < 6 && black(z) > 0.08 {
            z += 1;
        }
    }
    z
}

/// "PC wide (auto)" held between maps: the zoom is picked by [`auto_zoom`] when the shown map changes (an area or a
/// scene load) or the window does, and kept through frames that show no map (menu screens, black loads, the
/// prologue). Picking it from every frame's map made it jump between 2x and 4x whenever a menu opened over a small
/// map or a cutscene left and re-entered one.
///
/// "Steady" (the default, `max` false): at most one step above the window's base zoom (`auto_zoom` with no map), so
/// a tiny cutscene map shows with black margins instead of zooming in to fill the window - the zoom then only moves
/// between two values (Kid Buu's chapter: 240x160 scene maps between 480x400 ones jumped 3x / 4x / 6x at 1080p).
/// "Max" (`max` true): the map fills the window as far as the zoom allows (up to 6x).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoZoom {
    map: String,
    win: (u32, u32),
    zoom: u32,
    /// "Auto zoom: max" (no cap)
    pub max: bool,
}

impl AutoZoom {
    /// the zoom for this frame: `map` = the shown map's id and world size (None: no map this frame)
    pub fn zoom(&mut self, win_w: u32, win_h: u32, map: Option<(&str, (i32, i32))>) -> u32 {
        let win = (win_w, win_h);
        match map {
            Some((id, size)) if id != self.map || win != self.win || self.zoom == 0 => {
                self.zoom = auto_zoom(win_w, win_h, Some(size));
                self.map = id.to_string();
                self.win = win;
            }
            None if self.zoom == 0 || win != self.win => {
                self.zoom = auto_zoom(win_w, win_h, None);
                self.win = win;
            }
            _ => {}
        }
        if self.max {
            self.zoom
        } else {
            let base = auto_zoom(win_w, win_h, None);
            self.zoom.clamp(base, base + 1)
        }
    }
}

pub fn fit(mode: ViewMode, win_w: u32, win_h: u32, world: Option<(i32, i32)>) -> Fit {
    let (win_w, win_h) = (win_w.max(1), win_h.max(1));
    let max_z = (win_w / GBA_W as u32).min(win_h / GBA_H as u32).max(1);
    match mode {
        ViewMode::Gba { scale } => {
            Fit { view_w: GBA_W as u32, view_h: GBA_H as u32, scale: if scale > 0 { scale } else { max_z } }
        }
        ViewMode::Pc { zoom } => {
            let z = if zoom > 0 { zoom } else { auto_zoom(win_w, win_h, world) }.clamp(1, max_z);
            Fit { view_w: (win_w / z).max(1), view_h: (win_h / z).max(1), scale: z }
        }
    }
}

const L_OBJ: u8 = 4;
const L_BD: u8 = 5;

pub struct Renderer {
    sheets: HashMap<String, Option<Arc<Sheet>>>,
    maps: HashMap<String, Option<Arc<Map>>>,
    top: Vec<u32>,
    top_l: Vec<u8>,
    top_b: Vec<u16>,
    bot: Vec<u32>,
    bot_l: Vec<u8>,
    remap: Vec<u32>,
    /// Output, `0xAARRGGBB`, `w * h`.
    pub fb: Vec<u32>,
    pub w: usize,
    pub h: usize,
    /// Content problems (missing sheet / map), each reported once.
    pub errors: Vec<String>,
    pub draw_hud: bool,
    /// The game's UI (`ui/` content); `Some(None)` = not available (placeholder HUD is drawn instead).
    ui: Option<Option<ui::Ui>>,
    /// expanded colour -> the global palette's original RGB888 (`maps/palette.json`), for palette fades
    orig_pal: Option<HashMap<u32, u32>>,
    /// The area-name banner (`ui/banner.json`); `Some(None)` = content missing.
    banner: Option<Option<banner::Banner>>,
    pub blend_math: BlendMath,
    /// world-map data per map id (mode7.rs)
    mode7: HashMap<String, Option<Arc<wf_content::mode7::Mode7Data>>>,
    strings: Option<Vec<String>>,
    /// menu screens (`RenderView.menu`)
    menu: Option<menu::MenuRenderer>,
    /// the hit flash's colour maps (`ui/hit_flash.json`): colour -> colour per map; `Some(None)` = content missing
    flash: Option<Option<Vec<HashMap<u32, u32>>>>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn c5(c: u32, sh: u32) -> u32 {
    (c >> sh) & 0xFF
}

/// How blended colours are computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlendMath {
    /// Like mGBA (the reference the parity tests compare against): channels expanded to 8 bits
    /// (`c5 << 3 | c5 >> 2`) first, `min(255, (a * eva + b * evb) >> 4)`.
    #[default]
    Mgba,
    /// The GBA PPU: 5-bit channels, `min(31, (a * eva + b * evb) >> 4)`, then expanded.
    Hardware,
}

/// GBA alpha blend; inputs are expanded 8-bit colours (`c5 << 3 | c5 >> 2`).
#[inline]
fn blend(a: u32, b: u32, eva: u32, evb: u32, m: BlendMath) -> u32 {
    let mut out = 0xFF00_0000;
    for sh in [16, 8, 0] {
        let v = match m {
            BlendMath::Mgba => ((c5(a, sh) * eva + c5(b, sh) * evb) >> 4).min(255),
            BlendMath::Hardware => {
                let v = (((c5(a, sh) >> 3) * eva + (c5(b, sh) >> 3) * evb) >> 4).min(31);
                (v << 3) | (v >> 2)
            }
        };
        out |= v << sh;
    }
    out
}

/// BLDY brightness decrease.
#[inline]
fn darken(a: u32, evy: u32, _m: BlendMath) -> u32 {
    // BLDY brightness decrease on the 5-bit channels, then expanded - mGBA too (its screens show only expanded
    // values after a darken: real run 11-10 `dr2_007` f450, a scene's black fade at 9: 27 -> 12 = 99, not the
    // 8-bit 222 -> 98)
    let mut out = 0xFF00_0000;
    for sh in [16, 8, 0] {
        let x = c5(a, sh) >> 3;
        let v = x - ((x * evy) >> 4);
        out |= ((v << 3) | (v >> 2)) << sh;
    }
    out
}

/// `ui/hit_flash.json`: per colour-index map, the OBJ palette's colour of index i -> the colour of map[i] (the
/// first index of a colour wins; sheets carry expanded RGB, not indices)
fn load_flash(packs: &Packs) -> Option<Vec<HashMap<u32, u32>>> {
    let j = packs.value("ui/hit_flash.json").ok()?;
    let pal: Vec<u32> = j["obj_palette"].as_array()?.iter().map(|v| v.as_u64().unwrap_or(0) as u32).collect();
    let maps = j["maps"].as_array()?;
    Some(
        maps.iter()
            .map(|m| {
                let mut h = HashMap::new();
                for (i, v) in m.as_array().into_iter().flatten().enumerate().skip(1) {
                    if let (Some(&from), Some(&to)) = (pal.get(i), pal.get(v.as_u64().unwrap_or(0) as usize)) {
                        h.entry(from).or_insert(to);
                    }
                }
                h
            })
            .collect(),
    )
}

/// `maps/palette.json` (RGB888 as stored) keyed by the expanded colour images carry.
fn load_orig_palette(packs: &Packs) -> HashMap<u32, u32> {
    let mut m = HashMap::new();
    if let Ok(j) = packs.value("maps/palette.json") {
        // key: expanded colour | half << 24 (0 = BG 0..255, 1 = OBJ 256..511); `0xFF` << 24 = either half
        for (i, v) in j["rgb888"].as_array().into_iter().flatten().enumerate() {
            let Some(o) = v.as_str().and_then(|s| u32::from_str_radix(s.trim_start_matches('#'), 16).ok()) else { continue };
            let mut e = 0;
            for sh in [16, 8, 0] {
                let c5 = ((o >> sh) & 0xFF) >> 3;
                e |= ((c5 << 3) | (c5 >> 2)) << sh;
            }
            m.entry(e | ((i as u32 / 256) << 24)).or_insert(o);
            // by palette entry: 0x02 << 24 | half << 8 | index
            m.insert(0x0200_0000 | i as u32, o);
            m.entry(e | 0xFF00_0000).or_insert(o);
        }
    }
    m
}

/// the fade routine `0x08007074`: `(c * (256 - level) + target * level) >> 11` per channel (BGR555), expanded
fn palette_fade(o: u32, level: u32, target: u32) -> u32 {
    let l = level.min(256);
    let mut out = 0xFF00_0000;
    for sh in [16, 8, 0] {
        let c = (o >> sh) & 0xFF;
        let t = (target >> sh) & 0xFF;
        let v = ((c * (256 - l) + t * l) >> 11).min(31);
        out |= ((v << 3) | (v >> 2)) << sh;
    }
    out
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            sheets: HashMap::new(),
            maps: HashMap::new(),
            top: Vec::new(),
            top_l: Vec::new(),
            top_b: Vec::new(),
            bot: Vec::new(),
            bot_l: Vec::new(),
            remap: Vec::new(),
            fb: Vec::new(),
            w: 0,
            h: 0,
            errors: Vec::new(),
            draw_hud: true,
            ui: None,
            orig_pal: None,
            flash: None,
            banner: None,
            blend_math: BlendMath::default(),
            mode7: HashMap::new(),
            strings: None,
            menu: None,
        }
    }

    /// Drop cached assets (content / mods changed).
    pub fn clear_cache(&mut self) {
        self.menu = None;
        self.sheets.clear();
        self.maps.clear();
    }

    pub fn sheet(&mut self, packs: &Packs, key: &str) -> Option<Arc<Sheet>> {
        if let Some(s) = self.sheets.get(key) {
            return s.clone();
        }
        let s = match Sheet::load(packs, key) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                self.errors.push(format!("sheet {key}: {e}"));
                None
            }
        };
        self.sheets.insert(key.to_string(), s.clone());
        s
    }

    pub fn map(&mut self, packs: &Packs, id: &str) -> Option<Arc<Map>> {
        if let Some(m) = self.maps.get(id) {
            return m.clone();
        }
        // a field with no map (the New Game prologue's area 0-0): nothing to draw, not an error (the browser
        // console showed "map : missing content file maps//map.json")
        if id.is_empty() {
            return None;
        }
        let m = match Map::load(packs, id) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.errors.push(format!("map {id}: {e}"));
                None
            }
        };
        self.maps.insert(id.to_string(), m.clone());
        m
    }

    fn resize(&mut self, w: usize, h: usize) {
        let n = w * h;
        self.w = w;
        self.h = h;
        for v in [&mut self.top, &mut self.bot, &mut self.fb] {
            v.resize(n, 0);
        }
        self.top_l.resize(n, 0);
        self.bot_l.resize(n, 0);
        self.top_b.resize(n, 0);
    }

    /// World rectangle shown for local player `player` in a `w` x `h` view: centred on its 240x160 camera.
    pub fn view_origin(view: &RenderView, player: usize, w: usize, h: usize) -> (i32, i32, i32, i32) {
        let cam = view.cameras.get(player).or(view.cameras.first()).copied().unwrap_or_default();
        let vx = cam.x + GBA_W / 2 - w as i32 / 2;
        let vy = cam.y + GBA_H / 2 - h as i32 / 2;
        (vx, vy, cam.x, cam.y)
    }

    /// Compose the view for local player `player` into `self.fb` (`w` x `h` world pixels).
    pub fn render(&mut self, packs: &Packs, view: &RenderView, player: usize, w: usize, h: usize) -> &[u32] {
        self.resize(w, h);
        if let Some(mv) = view.menu.as_ref().filter(|m| m.overlay) {
            // a screen over the field (mini-games): the normal picture, then the screen's pixels on top
            let mv = mv.clone();
            let mut v2 = view.clone();
            v2.menu = None;
            self.render(packs, &v2, player, w, h);
            let mr = self.menu.get_or_insert_with(|| menu::MenuRenderer::load(packs));
            let pic = mr.render(packs, &mv, mv.fade.or(view.palette_fade), if mv.fade_black > 0 { mv.fade_black } else { view.fade_black });
            let (ox, oy) = ((w as i32 - GBA_W) / 2, (h as i32 - GBA_H) / 2);
            for y in 0..GBA_H {
                for x in 0..GBA_W {
                    let (fx, fy) = (ox + x, oy + y);
                    let c = pic[(y * GBA_W + x) as usize];
                    if c >> 24 != 0 && fx >= 0 && fy >= 0 && (fx as usize) < w && (fy as usize) < h {
                        self.fb[fy as usize * w + fx as usize] = c;
                    }
                }
            }
            return &self.fb;
        }
        if let Some(mv) = view.menu.as_ref() {
            // a menu screen replaces everything: the 240x160 picture centred in the view, black around it
            let mr = self.menu.get_or_insert_with(|| menu::MenuRenderer::load(packs));
            let pic = mr.render(packs, mv, mv.fade.or(view.palette_fade), if mv.fade_black > 0 || mv.fade.is_some() { mv.fade_black } else { view.fade_black });
            self.fb.fill(0xFF00_0000);
            let (ox, oy) = ((w as i32 - GBA_W) / 2, (h as i32 - GBA_H) / 2);
            for y in 0..GBA_H {
                for x in 0..GBA_W {
                    let (fx, fy) = (ox + x, oy + y);
                    if fx >= 0 && fy >= 0 && (fx as usize) < w && (fy as usize) < h {
                        self.fb[fy as usize * w + fx as usize] = pic[(y * GBA_W + x) as usize];
                    }
                }
            }
            return &self.fb;
        }
        let (vx, vy, cx, cy) = Self::view_origin(view, player, w, h);
        let map = view.map.as_ref().and_then(|m| self.map(packs, &m.id).map(|g| (g, (m.anim_frame, m.tile_frame.unwrap_or(m.anim_frame)), m.shake)));
        let shake_sub = view.map.as_ref().map(|m| m.shake_sub).unwrap_or((0, 0));
        // (the top byte is the colour's palette index for the fades (`put`): the backdrop is BG entry 0, which the
        // encoding cannot name - 0 = "find it by colour"; the map's 0xFF top byte read as BG entry 255 and faded
        // the margins of the zoomed-out view to an unrelated colour, e.g. brown in Southwest Forest's fades)
        let backdrop = map.as_ref().map(|(m, _, _)| m.backdrop).unwrap_or(0) & 0x00FF_FFFF;
        self.top.fill(backdrop);
        self.top_l.fill(L_BD);
        self.top_b.fill(0);
        self.bot.fill(backdrop);
        self.bot_l.fill(L_BD);

        // sprites resolved once: (priority, oam index, sheet, frame index, flip)
        let mut objs: Vec<(u8, usize, Arc<Sheet>, usize, bool)> = Vec::with_capacity(view.sprites.len());
        for (i, s) in view.sprites.iter().enumerate() {
            let Some(sheet) = self.sheet(packs, &s.sheet) else { continue };
            let (fi, flip) = match &s.frame {
                FrameSel::Frame { index, flip } => (*index as usize, *flip),
                FrameSel::Anim { anim, dir, t, looping } => match sheet.def.anim(anim).and_then(|a| a.step_at(*dir, *t, *looping)) {
                    Some((_, st)) => (st.frame as usize, st.flip),
                    None => continue,
                },
            };
            objs.push((s.priority.min(3), i, sheet, fi, flip ^ s.mirror));
        }

        // the world map (mode 7): its own BGs and OBJs; the view's sprites are world-space and not drawn
        let m7 = self.mode7_prepare(packs, view);
        if m7.is_some() {
            objs.clear();
        }

        // screen-space OBJs (HUD, dialogue box): (priority, x, y, w, h, ARGB)
        let mut screen: Vec<(u8, i32, i32, usize, usize, Vec<u32>)> = Vec::new();
        if self.ui.is_none() {
            let u = ui::Ui::load(packs);
            if let Err(e) = &u {
                self.errors.push(format!("ui: {e} (placeholder HUD)"));
            }
            self.ui = Some(u.ok());
        }
        let (ox, oy) = ((w as i32 - GBA_W) / 2, (h as i32 - GBA_H) / 2);
        if let Some(Some(u)) = &mut self.ui {
            if self.draw_hud {
                // the game keeps the HUD up behind text boxes; cutscenes hide it (`HudView.hidden`)
                if let Some(hv) = view.hud.get(player).filter(|h| !h.hidden) {
                    // anchored to the view's top-left corner (= the GBA position in the classic view)
                    // (`ui/layout.json` may move / re-anchor / hide it; the game's place by default)
                    if !u.layout.hud_hidden {
                        let (hx, hy) = u.layout.hud_at(w as i32, h as i32);
                        screen.push((u.layout.hud_priority, hx, hy, 80, 16, u.hud(packs, hv)));
                        // the stat-points mark beside it (OAM x 92 = the HUD's x + 84, the HUD's y, priority 1)
                        if let Some(px) = hv.stat_mark.and_then(|k| u.stat_mark(k)) {
                            screen.push((u.layout.hud_priority, hx + 84, hy, 16, 16, px));
                        }
                    }
                }
            }
            // boss HP bars: OAM x 160 (two 32x8 OBJs), priority 1; anchored to the view's top-right corner
            for bb in &view.boss_bars {
                if let Some(px) = u.boss_bar(bb.fill) {
                    screen.push((1, w as i32 - 80, bb.y, 64, 8, px));
                }
            }
            // the countdown / stopwatch display: OAM (168, 8), two 32x16 OBJs, priority 1; top-right anchored; over the boss bar
            if let Some(t) = view.timer {
                let mr = self.menu.get_or_insert_with(|| menu::MenuRenderer::load(packs));
                screen.push((1, w as i32 - 72, 8, 64, 16, mr.timer(packs, t)));
            }
            // damage numbers: 32x8 OBJs at their world place, priority 1 (draw 0x0800ACB6)
            for dn in &view.damage_numbers {
                let mr = self.menu.get_or_insert_with(|| menu::MenuRenderer::load(packs));
                screen.push((1, dn.x - vx, dn.y - vy, 32, 8, mr.damage_number(packs, dn.value, dn.crit)));
            }
            // a script's title text (`show_title_text`): OBJs at (x - w / 2, y - 16), priority 0
            for tv in &view.title {
                let mr = self.menu.get_or_insert_with(|| menu::MenuRenderer::load(packs));
                let (tw, px) = mr.title(packs, &tv.text);
                if tw > 0 {
                    screen.push((0, ox + tv.x - tw / 2, oy + tv.y - 16, tw as usize, 32, px));
                }
            }
            if let Some(bv) = &view.banner {
                if self.banner.is_none() {
                    self.banner = Some(banner::Banner::load(packs));
                }
                if let Some(Some(b)) = &mut self.banner {
                    if let Some(px) = b.frame(&bv.text, bv.t) {
                        // horizontally centred, bottom-anchored (32 px above the view's bottom edge, as on the GBA)
                        // (a layout's banner in the upper half anchors to the top edge instead)
                        let by = u.layout.banner_y;
                        let y = if by < GBA_H / 2 { by } else { h as i32 - (GBA_H - by) };
                        screen.push((banner::PRIORITY, ox, y, banner::W, banner::H, px));
                    }
                }
            }
            if let Some(dv) = &view.dialog {
                let anim = view.dialog_anim.unwrap_or_default();
                let hidden = anim.scale == 0xFF;
                // the box (+ portrait) span; zooming while it opens / closes
                let span = if hidden { None } else { u.dialog_span(packs, dv, anim.typed) };
                if let Some((sw, full, pieces)) = span {
                    let px = if anim.scale == 0 { full } else { ui::Ui::scale_span(&full, sw, &pieces, anim.scale as u32) };
                    // horizontally centred; top / bottom anchored to the view edges, middle centred
                    let y = match dv.position {
                        wf_view::DialogPos::Top => ui::Ui::dialog_y(dv.position),
                        wf_view::DialogPos::Middle => oy + ui::Ui::dialog_y(dv.position),
                        wf_view::DialogPos::Bottom => h as i32 - (GBA_H - ui::Ui::dialog_y(dv.position)),
                    };
                    let bx = ox + ui::DIALOG_CX - sw as i32 / 2;
                    screen.push((ui::DIALOG_PRIORITY, bx, y, sw, 64, px));
                    // the choice cursor at its option's pen (x - 16, y - 12; OAM 0x0801796A), in front of the box
                    if let (Some(sel), 0 | 8) = (dv.cursor, anim.scale) {
                        let box_x = bx + if dv.portrait.is_some() && dv.portrait_left { ui::PORTRAIT_W as i32 } else { 0 };
                        // the cursor object's blink (`0x0801794C`: +1 a draw, hidden while bit 3 is set), counted from
                        // its creation - the typing reaching the first option line (`DialogAnim::cursor_draws`)
                        let shown = anim.cursor_draws.is_some_and(|n| n & 8 == 0);
                        if let (Some((cx, cy)), Some(cur), true) = (u.dialog_cursor_spot(dv, sel, anim.typed), u.cursor(), shown) {
                            screen.push((ui::DIALOG_PRIORITY, box_x + cx - 16, y + cy - 12, 16, 16, cur));
                        }
                    }
                }
            }
        }

        if self.flash.is_none() {
            self.flash = Some(load_flash(packs));
        }
        for prio in (0..4u8).rev() {
            if let Some(f) = &m7 {
                self.draw_mode7_bgs(f, prio, ox, oy);
            }
            if let Some((m, af, shake)) = &map {
                // BGs of this priority, higher BG index further back (a script's BG control: hidden layers, priorities)
                let sl = view.map.as_ref().and_then(|v| v.layers.as_ref());
                let shown = |i: usize| sl.is_none_or(|l| l.visible.get(m.layers[i].bg as usize).copied().unwrap_or(true));
                let pr = |i: usize| sl.and_then(|l| l.priority.get(m.layers[i].bg as usize).copied().flatten()).unwrap_or(m.layers[i].priority);
                let mut bgs: Vec<usize> = (0..m.layers.len()).filter(|&i| shown(i) && pr(i) == prio).collect();
                bgs.sort_by(|&a, &b| m.layers[b].bg.cmp(&m.layers[a].bg));
                for li in bgs {
                    // BG0 shows the sphere effect's tilemap when one is up (it rewrites screenblock 28 whole)
                    if view.sphere.is_some() && m.layers[li].bg == 0 {
                        continue;
                    }
                    // a script's `set_bg_offset` adds to the BG's scroll (real route sb 6-6: s-script's BG2 by (-3, 3))
                    let (bx, by) = sl.and_then(|l| l.offset.get(m.layers[li].bg as usize).copied()).unwrap_or((0, 0));
                    self.draw_layer(m, li, *af, cx, cy, (shake.0 + bx, shake.1 + by), shake_sub, vx, vy);
                }
                // the sphere on BG0, at BG0's priority (2: BG0CNT 0x1C82 under it, real route 19-3; a script's BG
                // priority moves it - sb_041 21-25 f12,090: BG0CNT priority 3, Kid Buu's OBJs at 3 draw over it,
                // not blended), over the other BGs of its priority
                let bg0 = sl.and_then(|l| l.priority.first().copied().flatten()).unwrap_or(2);
                if view.sphere.is_some() && prio == bg0 {
                    let sp = view.sphere.as_ref().unwrap();
                    self.draw_sphere(m, sp, cx + shake.0, cy + shake.1, vx, vy);
                }
            }
            // OBJs of this priority, last OAM entry first
            for (p, i, sheet, fi, flip) in objs.iter().rev() {
                if *p == prio {
                    let s = &view.sprites[*i];
                    // the screen shake's offset moves the world's OBJs with the BG layers (real route sb 6-6 / 6-9 f4,694
                    // on: Gotenks' cut-short crit shake leaves (3, -3); the actors drawn 3 px off without it). The
                    // screen-space OBJs (HUD, boss bar, numbers' screen items) stay put
                    let (shx, shy) = view.obj_shake.or(map.as_ref().map(|m| m.2)).unwrap_or((0, 0));
                    self.draw_obj(sheet, *fi, *flip, s.x - vx - shx, s.y - vy - shy, s.effect);
                }
            }
            if let Some(f) = &m7 {
                self.draw_mode7_objs(f, prio, ox, oy);
            }
            for (p, x, y, sw, sh, px) in &screen {
                if *p == prio {
                    self.draw_argb(px, *sw, *sh, *x, *y);
                }
            }
        }

        // blending + output
        let (mut bld, mut eva, mut evb) = map.as_ref().map(|(m, _, _)| (m.bldcnt as u32, m.eva as u32, m.evb as u32)).unwrap_or((0, 16, 0));
        // a script's blend targets / weights over the map's (BLDCNT bit b / 8 + b, BLDALPHA)
        if let Some(sl) = view.map.as_ref().and_then(|v| v.layers.as_ref()) {
            for (b, t) in sl.blend.iter().enumerate() {
                if let Some(t) = t {
                    bld = (bld & !(1 << b) & !(1 << (8 + b))) | ((*t as u32 & 1) << b) | (((*t as u32 >> 1) & 1) << (8 + b));
                }
            }
            if let Some((a, c)) = sl.alpha {
                eva = a as u32;
                evb = c as u32;
            }
        }
        let mode = (bld >> 6) & 3;
        let fade = view.fade_black.min(16) as u32;
        let bm = self.blend_math;
        // palette fade: every colour as the fade routine left it in palette RAM, before blending
        if let Some(pf) = view.palette_fade.filter(|f| f.level > 0) {
            if self.orig_pal.is_none() {
                self.orig_pal = Some(load_orig_palette(packs));
            }
            let orig = self.orig_pal.as_ref().unwrap();
            let mut cache: HashMap<u64, u32> = HashMap::new();
            // the colour's palette entry when known (top byte), else the first entry of the layer's half (OBJ
            // pixels: OBJ half, BG pixels: BG half) with that colour
            let mut f = |e: u32, layer: u8| -> u32 {
                let half = (layer == L_OBJ) as u32;
                *cache.entry((half as u64) << 40 | e as u64).or_insert_with(|| {
                    let by_index = (e >> 24 != 0).then(|| orig.get(&(0x0200_0000 | (half << 8) | (e >> 24)))).flatten();
                    let o = by_index.or_else(|| orig.get(&((e & 0xFF_FFFF) | (half << 24)))).or_else(|| orig.get(&((e & 0xFF_FFFF) | 0xFF00_0000))).copied();
                    palette_fade(o.unwrap_or(e & 0xFF_FFFF), pf.level as u32, pf.target)
                })
            };
            for i in 0..w * h {
                self.top[i] = f(self.top[i], self.top_l[i]);
                self.bot[i] = f(self.bot[i], self.bot_l[i]);
            }
        }
        // a script fade out: the palette as it was, scaled per 5-bit channel toward the target
        if let Some(ps) = view.palette_scale.filter(|f| f.level > 0) {
            let l = ps.level.min(256) as u32;
            let f = |e: u32| -> u32 {
                let mut out = 0xFF00_0000;
                for sh in [16, 8, 0] {
                    let x = c5(e, sh) >> 3;
                    let t = c5(ps.target, sh) >> 3;
                    let v = ((x * (256 - l) + t * l) >> 8).min(31);
                    out |= ((v << 3) | (v >> 2)) << sh;
                }
                out
            };
            for i in 0..w * h {
                self.top[i] = f(self.top[i]);
                self.bot[i] = f(self.bot[i]);
            }
        }
        for i in 0..w * h {
            let tl = self.top_l[i];
            let mut c = self.top[i];
            let bl = self.bot_l[i];
            let second = bld >> 8 & (1 << bl) != 0;
            if self.top_b[i] != 0 {
                let b = self.top_b[i];
                if b == 0xFFFF {
                    if second {
                        c = blend(c, self.bot[i], eva.min(16), evb.min(16), bm);
                    }
                } else {
                    c = blend(c, self.bot[i], (b & 0xFF) as u32, (b >> 8) as u32, bm);
                }
            } else if mode == 1 && (bld & (1 << tl)) != 0 && second {
                c = blend(c, self.bot[i], eva.min(16), evb.min(16), bm);
            }
            if fade != 0 {
                c = darken(c, fade, bm);
            }
            self.fb[i] = c | 0xFF00_0000;
        }

        if self.draw_hud && matches!(self.ui, Some(None)) {
            self.draw_hud(view, player);
        }
        &self.fb
    }

    #[inline]
    /// `c` = RGB in the low 24 bits; the top byte carries the colour's palette index within the layer's half
    /// (BG 0..255 / OBJ 256..511), 0 = unknown — palette fades use it to find the original RGB888 exactly.
    fn put(&mut self, i: usize, c: u32, layer: u8, b: u16) {
        self.bot[i] = self.top[i];
        self.bot_l[i] = self.top_l[i];
        self.top[i] = c;
        self.top_l[i] = layer;
        self.top_b[i] = b;
    }

    #[allow(clippy::too_many_arguments)]
    /// `off`: added to the layer's scroll registers whatever its rate (the screen shake's layer offsets, a script's
    /// `set_bg_offset`) - not to the camera, which the view's origin cancels (real route sb 6-6 f4,694 on: the
    /// arena drawn (3, -3) off after Gotenks' crit shake); `sub`: the shake's sub-pixel part, into the scroll's sum
    /// (`Layer::scroll_with`: only a layer at another rate than the camera's rounds differently with it)
    #[allow(clippy::too_many_arguments)]
    fn draw_layer(&mut self, m: &Map, li: usize, (anim_frame, tile_frame): (u32, u32), cx: i32, cy: i32, off: (i32, i32), sub: (i32, i32), vx: i32, vy: i32) {
        let l = &m.layers[li];
        let ts = &m.tileset;
        // tile animation remap for this frame
        if self.remap.len() != ts.count as usize {
            self.remap = (0..ts.count).collect();
        }
        for (i, r) in self.remap.iter_mut().enumerate() {
            *r = i as u32;
        }
        // `anim_frame` = logic frames since the area load (the map's animation ticks). Overlapping animations: the
        // tile shows the most recent upload (ties: header order, as the tick loop uploads)
        let mut order: Vec<(u32, usize, usize)> = ts.anims.iter().enumerate().map(|(k, a)| {
            let (f, at) = a.after_ticks(k, tile_frame);
            (at, k, f)
        }).collect();
        order.sort();
        for &(_, ai, k) in &order {
            let a = &ts.anims[ai];
            for (i, &src) in a.src[k].iter().enumerate() {
                if let Some(r) = self.remap.get_mut((a.dest + i as u32) as usize) {
                    *r = src;
                }
            }
        }
        let (sx, sy) = l.scroll_with(cx, cy, anim_frame, sub);
        let (sx, sy) = (sx + off.0, sy + off.1);
        // screen pixel (px, py) of this view is camera-relative (vx - cx + px); layer pixel = that + scroll
        let ox = vx - cx + sx;
        let oy = vy - cy + sy;
        let (lw, lh) = ((l.w_tiles * 8) as i32, (l.h_tiles * 8) as i32);
        let wrap = l.kind == LayerKind::Wrap;
        let (w, h) = (self.w, self.h);
        let layer = l.bg;
        for py in 0..h {
            let mut ly = oy + py as i32;
            if wrap {
                ly = ly.rem_euclid(256);
            } else if ly < 0 || ly >= lh {
                continue;
            }
            let ty = (ly >> 3) as u32;
            let row_in = (ly & 7) as usize;
            let mut px = 0usize;
            while px < w {
                let mut lx = ox + px as i32;
                if wrap {
                    lx = lx.rem_euclid(256);
                } else if lx < 0 || lx >= lw {
                    // skip to the layer edge
                    if lx < 0 {
                        px += (-lx) as usize;
                    } else {
                        break;
                    }
                    continue;
                }
                let tx = (lx >> 3) as u32;
                let col = (lx & 7) as usize;
                let run = (8 - col).min(w - px);
                if let Some((t, fh, fv)) = l.tile(tx, ty) {
                    let t = self.remap.get(t as usize).copied().unwrap_or(t);
                    if t < ts.count {
                        let r = if fv { 7 - row_in } else { row_in };
                        let base = t as usize * 64 + r * 8;
                        let row = &ts.tiles[base..base + 8];
                        let o = py * w + px;
                        for k in 0..run {
                            let c = col + k;
                            let ix = row[if fh { 7 - c } else { c }];
                            if ix != 0 {
                                let colr = (ts.palette.get(ix as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (ix as u32) << 24;
                                self.put(o + k, colr, layer, 0);
                            }
                        }
                    }
                }
                px += run;
            }
        }
    }

    /// The BG sphere (`0x0803AA58`) on BG0: the lower-right quadrant's tiles from the midpoint circle (`0x0803AE10`:
    /// half-width per row from the centre), mirrored left (h-flip) and up (v-flip) about the centre; a tile whose
    /// last row's half-width - 3 covers it is a full tile, else per pixel: the colour up to `outer - rim`, colour 2 up
    /// to `outer` (rim = outer - inner(radius - 3) + jitter; no jitter at radius <= 3: no rim)
    fn draw_sphere(&mut self, m: &Map, sp: &wf_view::SphereView, cx: i32, cy: i32, vx: i32, vy: i32) {
        let circle = |r: i32| -> Vec<i32> {
            let mut b = vec![0i32; 129];
            let (mut x, mut y, mut d) = (0i32, r, 3 - 2 * r);
            while x < y {
                if (x as usize) < b.len() {
                    b[x as usize] = y;
                }
                if y >= 1 && ((y - 1) as usize) < b.len() {
                    b[(y - 1) as usize] = x;
                }
                if d < 0 {
                    d += 4 * x + 6;
                } else {
                    d += 4 * (x - y) + 10;
                    y -= 1;
                }
                x += 1;
            }
            b
        };
        let r = sp.radius.clamp(0, 128);
        let outer = circle(r);
        let mut rim = vec![0i32; 129];
        if r > 3 && !sp.jitter.is_empty() {
            let inner = circle(r - 3);
            for i in 0..r as usize {
                rim[i] = if i < (r - 3) as usize { sp.jitter.get(i).copied().unwrap_or(0) as i32 + outer[i] - inner[i] } else { outer[i] };
            }
        }
        let pal = |ix: u8| (m.tileset.palette.get(ix as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (ix as u32) << 24;
        let (fill, edge) = (pal(sp.colour), pal(2));
        // the quadrant pixel (qx, qy >= 0) -> colour index (0 = none)
        let at = |qx: i32, qy: i32| -> u8 {
            if qx >= 128 || qy >= 128 || qy >= r {
                return 0;
            }
            let (tr, tc) = (qy >> 3, qx >> 3);
            let last = outer[(tr * 8 + 7) as usize];
            if tc < (last - 3) >> 3 {
                return sp.colour;
            }
            let o = outer[qy as usize] - tc * 8;
            let x = qx & 7;
            if o <= 0 {
                return 0;
            }
            let f = o - rim[qy as usize];
            if x < f {
                sp.colour
            } else if x < o.min(8) {
                2
            } else {
                0
            }
        };
        let (w, h) = (self.w, self.h);
        // the centre on screen: the world point minus the camera (`cx` / `cy` with the shake), placed in this view
        // like the GBA screen (a wider view's centre 240x160 is the GBA screen: the wide view drew the sphere at the
        // GBA screen's coordinates - rkb_168 14-1 f2250, wide_view test)
        let (ox, oy) = (sp.x - cx + (w as i32 - GBA_W) / 2, sp.y - cy + (h as i32 - GBA_H) / 2);
        let _ = (vx, vy);
        for py in 0..h as i32 {
            let dy = py - oy;
            let qy = if dy >= 0 { dy } else { -1 - dy };
            if qy >= r {
                continue;
            }
            for px in 0..w as i32 {
                let dx = px - ox;
                let qx = if dx >= 0 { dx } else { -1 - dx };
                if qx >= r + 8 {
                    continue;
                }
                let c = at(qx, qy);
                if c != 0 {
                    self.put((py as usize) * w + px as usize, if c == 2 { edge } else { fill }, 0, 0);
                }
            }
        }
    }

    /// An opaque-where-alpha OBJ already in ARGB at screen (x, y).
    fn draw_argb(&mut self, px: &[u32], sw: usize, sh: usize, x: i32, y: i32) {
        for j in 0..sh {
            let yy = y + j as i32;
            if yy < 0 || yy >= self.h as i32 {
                continue;
            }
            for i in 0..sw {
                let xx = x + i as i32;
                let c = px[j * sw + i];
                if xx < 0 || xx >= self.w as i32 || c >> 24 == 0 {
                    continue;
                }
                self.put(yy as usize * self.w + xx as usize, c, L_OBJ, 0);
            }
        }
    }

    fn draw_obj(&mut self, sheet: &Sheet, fi: usize, flip: bool, x: i32, y: i32, eff: SpriteEffect) {
        let Some(f) = sheet.def.frames.get(fi) else { return };
        let img = &sheet.image;
        let b: u16 = match eff {
            SpriteEffect::SemiTransparent => 0xFFFF,
            SpriteEffect::Blend { eva, evb } => (eva.min(16) as u16) | (evb.min(16) as u16) << 8,
            _ => 0,
        };
        let white = eff == SpriteEffect::White;
        let vflip = eff == SpriteEffect::VFlip;
        let fmap = match eff {
            SpriteEffect::Flash(k) => self.flash.as_ref().and_then(|f| f.as_ref()).and_then(|f| f.get(k as usize)).cloned(),
            _ => None,
        };
        let (w, h) = (self.w as i32, self.h as i32);
        for j in 0..f.h as i32 {
            let sy = if vflip { y + f.anchor.y - 1 - j } else { y - f.anchor.y + j };
            if sy < 0 || sy >= h {
                continue;
            }
            let src_row = ((f.y as i32 + j) as u32 * img.w) as usize;
            for i in 0..f.w as i32 {
                let sx = if flip { x + f.anchor.x - 1 - i } else { x - f.anchor.x + i };
                if sx < 0 || sx >= w {
                    continue;
                }
                let c = img.px[src_row + (f.x as i32 + i) as usize];
                if c >> 24 == 0 {
                    continue;
                }
                // sheets are RGBA (palette swaps applied): the palette index is unknown (alpha 0, see `put`)
                let c = if white { 0x00FF_FFFF } else { c & 0xFF_FFFF };
                let c = match &fmap {
                    Some(m) => m.get(&c).copied().unwrap_or(c),
                    None => c,
                };
                self.put(sy as usize * self.w + sx as usize, c, L_OBJ, b);
            }
        }
    }

    /// Placeholder HUD (the game's HUD graphics are not extracted yet): name, level, HP / EP bars, top-left.
    fn draw_hud(&mut self, view: &RenderView, player: usize) {
        let Some(hud) = view.hud.get(player) else { return };
        let (w, h) = (self.w, self.h);
        let fb = &mut self.fb;
        let label = format!("{} LV{}", hud.name, hud.level);
        font::draw_text(fb, w, h, 4, 4, &label, 0xFFFF_FFFF);
        let bar = |fb: &mut [u32], y: i32, v: i32, max: i32, col: u32| {
            let bw = 60;
            font::fill_rect(fb, w, h, 4, y, bw + 2, 5, 0xFF10_1010);
            let fill = if max > 0 { (v.clamp(0, max) as i64 * bw as i64 / max as i64) as i32 } else { 0 };
            font::fill_rect(fb, w, h, 5, y + 1, fill, 3, col);
        };
        bar(fb, 14, hud.hp, hud.hp_max, 0xFF30_D040);
        bar(fb, 21, hud.ep, hud.ep_max, 0xFF40_90F0);
    }
}

/// FNV-1a over the framebuffer (golden-image tests, cross-platform determinism checks).
pub fn hash_fb(fb: &[u32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &p in fb {
        for b in p.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

#[cfg(test)]
mod auto_zoom_tests {
    use super::*;

    #[test]
    fn held_between_maps() {
        let mut a = AutoZoom { max: true, ..Default::default() };
        let big = Some(("area_00_050", (3584, 2560)));
        let small = Some(("area_20_009", (240, 160)));
        assert_eq!(a.zoom(1920, 1080, big), 2);
        // a menu / a black load over the big map: no map, the zoom stays
        assert_eq!(a.zoom(1920, 1080, None), 2);
        // the small map picks its own zoom on entry and keeps it through a menu over it
        let z = a.zoom(1920, 1080, small);
        assert_eq!(z, auto_zoom(1920, 1080, Some((240, 160))));
        assert_eq!(a.zoom(1920, 1080, None), z);
        assert_eq!(a.zoom(1920, 1080, small), z);
        // a resized window re-picks
        assert_eq!(a.zoom(1280, 800, small), auto_zoom(1280, 800, Some((240, 160))));
    }

    #[test]
    fn steady_caps_one_step_above_the_base() {
        let mut a = AutoZoom::default();
        assert!(!a.max);
        let base = auto_zoom(1920, 1080, None);
        assert_eq!(a.zoom(1920, 1080, Some(("big", (3584, 2560)))), base);
        // a 240x160 scene map: 6x uncapped at 1080p, base + 1 steady
        assert_eq!(auto_zoom(1920, 1080, Some((240, 160))), 6);
        assert_eq!(a.zoom(1920, 1080, Some(("tiny", (240, 160)))), base + 1);
        assert_eq!(a.zoom(1920, 1080, Some(("mid", (480, 400)))), base + 1);
        assert_eq!(a.zoom(1920, 1080, Some(("big", (3584, 2560)))), base);
        a.max = true;
        assert_eq!(a.zoom(1920, 1080, Some(("tiny", (240, 160)))), 6);
    }
}
