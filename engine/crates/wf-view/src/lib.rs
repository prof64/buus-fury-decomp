//! The contract between a deterministic simulation (`wf-core`'s World, or the frontend's demo world) and the
//! frontend (`wf-render`, `wf-app`, the native and web shells). See `engine/docs/frontend.md`.
//!
//! * The frontend owns the clock: it calls [`Sim::step`] once per game frame (59.7275 Hz) with every player's
//!   buttons. That is the only way the simulation state changes.
//! * After a step the frontend asks for a [`RenderView`] ([`Sim::view`], `&self`): plain data — which map, which
//!   sprite frames where, per-player cameras and HUD values. Rendering never touches simulation state, so the
//!   same state can be rendered any number of times (rewind / replay scrubbing / spectators / split screen).
//! * Side effects that are not state (sound, rumble, ...) leave the step as [`SimEvent`]s. They are a pure
//!   function of (state, inputs), so a replay reproduces them; the frontend may drop them while fast-forwarding.
//!
//! No dependencies, so `wf-core` can implement [`Sim`] without pulling in any frontend code.

/// GBA KEYINPUT bit layout (bit set = pressed; the hardware register is active-low, this is not).
pub mod buttons {
    pub const A: u16 = 1 << 0;
    pub const B: u16 = 1 << 1;
    pub const SELECT: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const RIGHT: u16 = 1 << 4;
    pub const LEFT: u16 = 1 << 5;
    pub const UP: u16 = 1 << 6;
    pub const DOWN: u16 = 1 << 7;
    pub const R: u16 = 1 << 8;
    pub const L: u16 = 1 << 9;
    pub const DPAD: u16 = RIGHT | LEFT | UP | DOWN;
    /// Names in bit order (for bindings UIs / config files).
    pub const NAMES: [&str; 10] = ["A", "B", "Select", "Start", "Right", "Left", "Up", "Down", "R", "L"];
}

/// One player's buttons for one frame (see [`buttons`]).
pub type Buttons = u16;

/// GBA screen = the game's logical camera size.
pub const GBA_W: i32 = 240;
pub const GBA_H: i32 = 160;
/// Frames per second of the original hardware (16.78 MHz / 280 896 cycles per frame).
pub const GBA_FPS: f64 = 16_777_216.0 / 280_896.0;

/// Everything the frontend needs to draw one frame. Built by [`Sim::view`]; contains no references into the
/// simulation. Reuse one instance across frames ([`RenderView::clear`]) to avoid allocations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderView {
    /// Simulation frame number this view was taken at.
    pub frame: u64,
    /// The map shown (BG layers) — `None` = backdrop only.
    pub map: Option<MapView>,
    /// OBJ layer: every sprite to draw this frame, in OAM order (index 0 is drawn on top among equal
    /// priorities — the renderer draws the list back to front).
    pub sprites: Vec<SpriteDraw>,
    /// One logical 240x160 camera per local player (index = player slot). Each local viewport is centred on
    /// its player's camera; zoomed-out views show more around it, the game logic only ever sees this rectangle.
    pub cameras: Vec<CameraView>,
    /// HUD values per player slot.
    pub hud: Vec<HudView>,
    /// Screen brightness fade: 0 = normal, 16 = black (GBA BLDY semantics, applied after composition).
    pub fade_black: u8,
    /// The dialogue box, if one is open (drawn above everything, like the game's priority-0 OBJs).
    pub dialog: Option<DialogView>,
    /// The area-name banner shown on area entry (`ui/banner.json`), if one is up.
    pub banner: Option<BannerView>,
    /// The game's palette fade (`0x08007074`: palette RAM = `(c * (256 - level) + target * level) >> 11` from the
    /// original RGB888 colours), used by area transitions (brightness 256 -> 0 by 8 per frame) and screen fades.
    /// Applied to every colour before composition; `None` = no fade. Takes precedence over `fade_black`.
    pub palette_fade: Option<PaletteFade>,
    /// World-map (mode 7) extras the renderer cannot derive from `MapView.mode7` alone; `None` = defaults (see
    /// [`WorldMapView`]).
    pub world_map: Option<WorldMapView>,
    /// The dialogue box's animation (open / close zoom, typewriter); `None` = fully open, all text shown.
    pub dialog_anim: Option<DialogAnim>,
    /// A full-screen screen drawn by code (pause menu pages, shop, save / load, character switch, title, game
    /// over, credits, mini-games, map screen): when set the renderer shows only this (plus `palette_fade` /
    /// `fade_black`), not the map, sprites, HUD or dialogue.
    pub menu: Option<MenuView>,
}

/// A screen the game draws by code into 8bpp surfaces (see `engine/docs/ui.md` "Menus"). Everything is in the
/// GBA's 240x160 screen; colours are indices of the UI palette (`ui/palette.json`, `0x0805652C`, loaded into both
/// palette halves) unless `palette` overrides entries.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuView {
    /// BG layers, drawn back to front (equal priority: later on top); index 0 of a layer is transparent.
    pub layers: Vec<MenuLayer>,
    /// OBJ sprites in OAM order (index 0 on top among equal priorities).
    pub objs: Vec<MenuObj>,
    /// palette entries replacing the UI palette's: (index 0..512, 0xRRGGBB); 256.. = OBJ half
    pub palette: Vec<(u16, u32)>,
    /// backdrop colour index (palette entry shown where every layer is transparent)
    pub backdrop: u16,
    /// the screen's own palette fade (`0x08007074`) / BLDY fade; `RenderView.palette_fade` / `fade_black` apply when
    /// these are not set
    pub fade: Option<PaletteFade>,
    pub fade_black: u8,
    /// the screen is drawn over the field picture (mini-games: the world stays on screen, the game adds OBJs): no
    /// backdrop, the renderer composes the normal view first, then these layers / OBJs on top
    pub overlay: bool,
    /// BLDCNT / BLDALPHA alpha blending (the credits' fades): None = off
    pub blend: Option<MenuBlend>,
}

/// GBA alpha blending: where the top pixel's layer is a first target and the pixel under it a second target,
/// colour = min(255, (top * eva + under * evb) >> 4) per 8-bit channel (mGBA's math, docs/frontend.md)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MenuBlend {
    /// first targets: bit i = `layers[i]`, bit 31 = OBJs
    pub first: u32,
    /// second targets: bit i = `layers[i]`, bit 31 = OBJs, bit 30 = the backdrop
    pub second: u32,
    pub eva: u8,
    pub evb: u8,
}

/// One background: a `w` x `h` 8bpp surface built from `ops` (in order), shown at screen (x, y).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuLayer {
    pub w: u16,
    pub h: u16,
    pub x: i32,
    pub y: i32,
    pub priority: u8,
    pub ops: Vec<MenuOp>,
}

/// The game's software drawing primitives (re/buu/menu, `engine/docs/ui.md`).
#[derive(Clone, Debug, PartialEq)]
pub enum MenuOp {
    /// `0x08004F1C`: rows alternate colour 0x01 / 0x30 starting with 0x01.
    Background { x: i32, y: i32, w: i32, rows: i32 },
    /// `0x08004F50`: the bevelled yellow panel with its 3-px shadow; rect (x0, y0)-(x1, y1).
    Panel { x0: i32, y0: i32, x1: i32, y1: i32 },
    /// fill (x0, y0)-(x1, y1) (exclusive) with a colour
    Fill { x0: i32, y0: i32, x1: i32, y1: i32, colour: u8 },
    /// a single pixel
    Pixel { x: i32, y: i32, colour: u8 },
    /// one line of text, already aligned: glyph cells start at pen x, their top at `y - 12`; 0x0008 n codes switch
    /// the colour; `shadow` = the glyph routine `0x0803F5A4` (each glyph also sets colour `shadow` where its pixels
    /// shifted by (+1, +1) fall and it has none), else `0x0803F444`. Pixels outside `clip` (x0, y0, x1, y1) stay.
    Text { font: MenuFont, x: i32, y: i32, colour: u8, shadow: Option<u8>, clip: [i32; 4], text: Vec<u16> },
    /// frame `frame` of a content picture (`path` = an indexed PNG in the pack, e.g. `ui/menu/button_a.png`; frames
    /// side by side, `fw` x `fh` each) copied to (x, y) (`0x0803F922`: index 0 skipped when `key0`)
    Image { path: String, frame: u16, fw: u16, fh: u16, x: i32, y: i32, key0: bool },
    /// one text line with the outlined glyph routine `0x0803F70C`: each glyph covers (x - 1 .. x + w, top ..
    /// top + 17); glyph pixels take the colour, their 4-neighbours (left / right / above / below) `outline`
    TextOutline { font: MenuFont, x: i32, y: i32, colour: u8, outline: u8, clip: [i32; 4], text: Vec<u16> },
    /// the layer drawn so far becomes a texture shown as affine OBJs (no double size, PA = PD = `pa`, PB = PC = 0):
    /// each piece (src x, src y, w, h, dst x, dst y) samples `(pa * (screen - centre) >> 8) + w / 2` from its rect;
    /// the first piece is on top (OAM order); the layer is cleared to transparent first
    AffineObjs { pieces: Vec<[i32; 6]>, pa: i32 },
    /// `0x0803F9D4`: every pixel of (x0, y0)-(x1, y1) (exclusive) replaced by `table[pixel]` (the title menu's
    /// darkened box)
    Remap { x0: i32, y0: i32, x1: i32, y1: i32, table: Vec<u8> },
    /// one 8x8 tile of a tile sheet (`path`: tiles 8x8 left to right, rows top to bottom) at (x, y), index 0 skipped
    Tile { path: String, tile: u16, x: i32, y: i32, hflip: bool, vflip: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MenuFont {
    Small,
    Dialog,
    Digits,
    #[default]
    Menu,
    Bold,
}

impl MenuFont {
    pub fn id(self) -> &'static str {
        match self {
            MenuFont::Small => "small",
            MenuFont::Dialog => "dialog",
            MenuFont::Digits => "digits",
            MenuFont::Menu => "menu",
            MenuFont::Bold => "bold",
        }
    }
}

/// An OBJ of a menu screen: frame `frame` of the indexed picture `path` (frames side by side, `fw` x `fh`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuObj {
    pub path: String,
    pub frame: u16,
    pub fw: u16,
    pub fh: u16,
    pub x: i32,
    pub y: i32,
    pub priority: u8,
    pub hflip: bool,
    pub vflip: bool,
    /// palette bank offset added to the picture's indices (OBJ half = 256)
    pub pal: u16,
    /// a sprite sheet (`sprites/<id>`, `effects/<id>`) instead of `path`: `frame` = the sheet's frame index,
    /// (x, y) = the anchor position (the frame's anchor lands there; `hflip` mirrors about it like the field)
    pub sheet: String,
    /// the object's colour remap (`0x0800EECC`: OBJ pixel index -> table[index]; e.g. the silhouette `0x0805682C`);
    /// empty = none
    pub remap: Vec<u8>,
}

/// The dialogue box OBJs zoom about the box centre while opening / closing (affine PA = PD = 2048 / scale) and
/// the text types in one visible character per frame (`wf_script_core::text::typing` counts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DialogAnim {
    /// 1..=8 (8 = full size)
    pub scale: u8,
    /// visible characters of the current page drawn so far (colour pairs and an option's "- " take none);
    /// `None` = all
    pub typed: Option<u32>,
}

/// Exact world-map presentation state (docs/frontend.md "World map"). Every field is optional; without it the
/// renderer uses `MapView.mode7` for everything, derives the flyer from that camera, shows the props flagged
/// visible in content and the mini-map box of the map's on-enter script.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WorldMapView {
    /// The camera the BG2 scanline table / sky scroll were built from: the game builds them before its camera
    /// update, so the picture shows the previous frame's camera on the ground while billboards / the flyer use
    /// the current one.
    pub bg_camera: Option<Mode7View>,
    /// The flyer: x, y (24.8 world px), z (22.10, up), its sprite sheet (`sprites/<id>`), frame and step flip; the
    /// renderer draws it (with halo and shadow) projected — `SpriteDraw`s of the player are ignored on the world map.
    pub flyer: Option<FlyerView>,
    /// Props shown / hidden by scripts (`prop_show` / `prop_hide`), overriding the content flag.
    pub props_shown: Vec<u16>,
    pub props_hidden: Vec<u16>,
    /// The mini-map marker box (`spawn_box(x0, y0, x1, y1)`), None = the content default.
    pub minimap_box: Option<[i32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FlyerView {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub sheet: String,
    pub frame: u16,
    pub flip: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PaletteFade {
    /// 0 = normal .. 256 = the target colour
    pub level: u16,
    /// target colour, `0xRRGGBB` (black = 0)
    pub target: u32,
}

/// The area-name banner (object `0x0800BF34`, vt `0x0804B528`): created by EnterArea after the player and the HUD
/// when the new area's name string differs from the last one shown (and is not 0); not on the 3D world map.
/// Shown for 239 frames: `t` = frames since its first draw (0..=238); from t = 223 the strip collapses vertically.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BannerView {
    /// The area name (string table text, e.g. "Southwest Forest").
    pub text: String,
    pub t: u32,
}

/// An open dialogue box (`ui/dialog.json`): the renderer word-wraps `text` with the game's line fitter, pages it
/// by the style's line count and draws page `page`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DialogView {
    /// Text as the game stores it: `\n` new line, `\u{b}` page break, `\u{8}` + char(n) = colour n.
    pub text: String,
    /// Box style (0 = 4 lines, 1 = 2 lines).
    pub style: u8,
    pub page: u16,
    /// Box placement (the text's position code `!` top, `@` middle, `#` bottom; else from the speaker, see
    /// `docs/ui.md` §3).
    pub position: DialogPos,
    /// Speaker portrait: the `portrait_index` of `portraits/index.json` (already resolved from the text node's
    /// `arg`: 1..6 = the leader's form slot, >= 8 = arg - 8). `None` = plain centred box. With a portrait the box
    /// and the 64x64 portrait share the screen's 224-px span and the text is left / top aligned (unless it starts
    /// with `^`).
    pub portrait: Option<u16>,
    /// The portrait sits left of the box (else right).
    pub portrait_left: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DialogPos {
    Top,
    Middle,
    #[default]
    Bottom,
}

impl RenderView {
    pub fn clear(&mut self) {
        self.frame = 0;
        self.map = None;
        self.sprites.clear();
        self.cameras.clear();
        self.hud.clear();
        self.fade_black = 0;
        self.dialog = None;
        self.palette_fade = None;
        self.banner = None;
        self.world_map = None;
        self.dialog_anim = None;
        self.menu = None;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapView {
    /// Content id of the map (`maps/<id>/map.json`), e.g. `area_08_003`.
    pub id: String,
    /// Frames since the map was loaded: drives tile animations and wrap-layer drift exactly like the game.
    pub anim_frame: u32,
    /// Screen shake added to every BG scroll (px), like the game's shake offset.
    pub shake: (i32, i32),
    /// The world map (area 0-50, map header "3D" flag): the mode-7 ground camera; None on 2D maps.
    pub mode7: Option<Mode7View>,
    /// Script BG control over the map's own (DISPCNT / BGxCNT / BLDCNT / BLDALPHA writes); None = the map's.
    pub layers: Option<BgLayers>,
}

/// BG state a script set (wf-core `scene::Layers`): per BG0..3 shown, priority, blend targets (bit 0 first target,
/// bit 1 second); the blend weights.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BgLayers {
    pub visible: [bool; 4],
    pub priority: [Option<u8>; 4],
    pub blend: [Option<u8>; 4],
    pub alpha: Option<(u8, u8)>,
}

/// The world map's 3D camera (map object `0x030019A4`, re/buu/notes.md "Field: world map"): the renderer projects
/// the 3584x2560 ground (layer kind 0x08007B63) per scanline below the horizon (HBlank BG2 PA/PC/X/Y, `0x0802ED98`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Mode7View {
    /// eye position (22.10 of world px x4: `(x << 2) - ...`), height (22.10), heading (22.10, 256 << 10 = a turn,
    /// 0 north, + clockwise), pitch (22.10 angle)
    pub x: i32,
    pub y: i32,
    pub height: i32,
    pub heading: i32,
    pub pitch: i32,
    /// the bottom label's string id (message triggers 0x17), None = none
    pub label: Option<i32>,
}

/// Which pixels of a sprite sheet to draw.
#[derive(Clone, Debug, PartialEq)]
pub enum FrameSel {
    /// A frame index of the sheet (`frames[i]` in sprite.json / effect.json) with the step's flip.
    Frame { index: u16, flip: bool },
    /// Let the renderer resolve the animation step from content timing: `anim` name, direction
    /// (`down`/`up`/`left`/`right`/`any`), `t` = frames since the animation started.
    Anim { anim: String, dir: u8, t: u32, looping: bool },
}

/// One OBJ draw. Coordinates are world pixels of the entity position (the frame's anchor lands there).
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteDraw {
    /// Simulation entity id (debug overlays / picking; not used for drawing).
    pub entity: u32,
    /// Content key of the sheet: `sprites/<id>` or `effects/<id>`.
    pub sheet: String,
    pub frame: FrameSel,
    pub x: i32,
    pub y: i32,
    /// GBA OBJ priority 0..3: drawn above BGs of the same or a higher number, below BGs with a lower number
    /// (entities use 3, so BG0 tree tops at priority 2 cover them, see docs/frontend.md).
    pub priority: u8,
    /// Extra horizontal mirror on top of the step's own flip (GBA OBJ h-flip around the anchor).
    pub mirror: bool,
    pub effect: SpriteEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SpriteEffect {
    #[default]
    None,
    /// Hit flash: every opaque pixel white.
    White,
    /// GBA semi-transparent OBJ: alpha blended over what is below with the map's EVA/EVB.
    SemiTransparent,
    /// Explicit blend `eva/16 * sprite + evb/16 * below`.
    Blend { eva: u8, evb: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CameraView {
    /// Top-left of the 240x160 logical camera in world pixels.
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HudView {
    pub name: String,
    pub level: u32,
    pub hp: i32,
    pub hp_max: i32,
    pub ep: i32,
    pub ep_max: i32,
    /// Super Saiyan / SS3 gauges (character record +0x1E / +0x20), drawn in the EP bar.
    pub ss: i32,
    pub ss3: i32,
    /// Experience line steps 0..72 (`0x0801363E(player, 72)`).
    pub xp72: u8,
    /// Selected skill id (icon `icons/skills/skill_NN.png`), `None` = empty slot.
    pub skill: Option<u8>,
    /// Use the `_alt` icon (not enough EP for the skill).
    pub skill_alt: bool,
    /// The game hides the HUD during dialogue / cut-scenes.
    pub hidden: bool,
    /// The stat-points mark ("STAT" + arrow, 16x16 at screen x 92 beside the HUD) while the leader has points to
    /// spend: its animation frame 0..3 (`ui/hud/stat_mark.png`), `None` = no mark.
    pub stat_mark: Option<u8>,
}

/// Non-state outputs of a step, in emission order.
#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    Audio(AudioEvent),
    /// Free-form, for debugging tools.
    Debug(String),
}

/// Commands for the sound driver (`wf-audio` semantics: 15 music tracks + 16 SFX voices, an SFX request is
/// dropped when all 16 voices are busy).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioEvent {
    PlaySong(u16),
    StopSong,
    /// `volume` 0..64, `pan` -64..64.
    PlaySfx { id: u8, volume: u8, pan: i8 },
    StopSfx(u8),
    StopAllSfx,
    /// Options-menu volumes 0..8 (game defaults: music 4, SFX 6).
    SetMasterVolumes { music: u8, sfx: u8 },
}

impl AudioEvent {
    /// Compact 4-word encoding for crossing thread / wasm boundaries (kind, a, b, c).
    pub fn encode(self) -> [i32; 4] {
        match self {
            AudioEvent::PlaySong(id) => [1, id as i32, 0, 0],
            AudioEvent::StopSong => [2, 0, 0, 0],
            AudioEvent::PlaySfx { id, volume, pan } => [3, id as i32, volume as i32, pan as i32],
            AudioEvent::StopSfx(id) => [4, id as i32, 0, 0],
            AudioEvent::StopAllSfx => [5, 0, 0, 0],
            AudioEvent::SetMasterVolumes { music, sfx } => [6, music as i32, sfx as i32, 0],
        }
    }
    pub fn decode(w: [i32; 4]) -> Option<AudioEvent> {
        Some(match w[0] {
            1 => AudioEvent::PlaySong(w[1] as u16),
            2 => AudioEvent::StopSong,
            3 => AudioEvent::PlaySfx { id: w[1] as u8, volume: w[2] as u8, pan: w[3] as i8 },
            4 => AudioEvent::StopSfx(w[1] as u8),
            5 => AudioEvent::StopAllSfx,
            6 => AudioEvent::SetMasterVolumes { music: w[1] as u8, sfx: w[2] as u8 },
            _ => return None,
        })
    }
}

/// A deterministic simulation the frontend can drive. `wf-core`'s World implements this (directly or through a
/// thin adapter in `wf-app`); so does the frontend's demo world.
pub trait Sim {
    /// Advance one frame. `inputs[p]` = player p's buttons (missing players = 0). Events are appended.
    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>);
    /// Describe the current state for drawing. Must not change the state.
    fn view(&self, out: &mut RenderView);
    /// Frame counter (number of steps taken).
    fn frame(&self) -> u64;
    /// Hash of the full state (desync checks / replays); 0 = not implemented.
    fn state_hash(&self) -> u64 {
        0
    }
    /// Serialized state for save states / rewind / rollback; `None` = not supported.
    fn snapshot(&self) -> Option<Vec<u8>> {
        None
    }
    /// Restore a [`Sim::snapshot`]. Returns false if unsupported or invalid.
    fn restore(&mut self, _snapshot: &[u8]) -> bool {
        false
    }
    /// Number of player slots this simulation reads from `inputs`.
    fn players(&self) -> usize {
        1
    }
    /// The concrete simulation, for tools that understand it (training overlays, ghosts); `None` = opaque.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
    /// What to do while the local player is in a menu: `None` = stop stepping (a local game pauses); `Some(b)` =
    /// keep stepping with buttons `b` for the local player (an online match cannot wait for one player).
    fn paused_input(&self) -> Option<Buttons> {
        None
    }
    /// Display frames the original hardware would have spent without a new logic frame after the last step (an
    /// area load, ...). Presentation only: a frontend's optional "original pacing" holds the picture that long;
    /// never part of the simulation state (replays / netplay ignore it).
    fn lag_hint(&self) -> u32 {
        0
    }
}
