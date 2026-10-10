//! ROM structures: area records, map headers, layer resources, tile animations.
//! Field documentation: `engine/docs/maps.md`.

use wf_rom::Rom;

pub const AREA_TABLE: u32 = 0x0808_E2E0;
pub const AREA_COUNT: u32 = 452;
pub const AREA_SIZE: u32 = 0x38;
/// Global 512-colour palette (BG 0..255, OBJ 256..511) as RGB888 triplets, faded into palette RAM by
/// `0x08007074` every frame. Every map uses the BG half unchanged.
pub const GLOBAL_PALETTE_RGB888: u32 = 0x0807_01B0;
/// BGxCNT base values per display mode (`0x080716C4 + 0x14 + 8 * header.mode`), priority added from the header.
pub const BGCNT_TABLE: u32 = 0x0807_16D8;

/// Layer resource kinds, by the resource's create function (+0x00).
pub const RES_TILEMAP: u32 = 0x0800_8087; // chunked 8bpp text BG (vtable 0x08049410)
pub const RES_EMPTY: u32 = 0x0800_7A41; // disabled BG
pub const RES_WRAP: u32 = 0x0800_897B; // one 32x32 screen, wraps, auto-scrolls (vtable 0x080498B0)

#[derive(Clone, Debug)]
pub struct Area {
    pub index: u32,
    pub addr: u32,
    pub world: u8,
    pub area: u8,
    pub variants: u8,
    pub n_cond_objects: u8,
    pub n_objects: u8,
    pub n_objects2: u8,
    pub flag8: u32,
    pub name_id: u32,
    pub cond_objects: u32,
    pub objects: u32,
    pub objects2: u32,
    pub song: u32,
    pub word28: u32,
    pub headers_list: u32,
    pub script: u32,
    pub word34: u32,
    pub headers: Vec<u32>,
}

impl Area {
    pub fn key(&self) -> String {
        format!("{}-{}", self.world, self.area)
    }
    pub fn label(&self) -> String {
        format!("area_{:02}_{:03}", self.world, self.area)
    }
}

pub fn areas(rom: &Rom) -> Vec<Area> {
    (0..AREA_COUNT)
        .map(|i| {
            let a = AREA_TABLE + AREA_SIZE * i;
            let variants = rom.u8(a + 2);
            let hl = rom.u32(a + 0x2C);
            let headers = if rom.isptr(hl) { (0..variants as u32).map(|k| rom.u32(hl + 4 * k)).collect() } else { vec![] };
            Area {
                index: i,
                addr: a,
                world: rom.u8(a),
                area: rom.u8(a + 1),
                variants,
                n_cond_objects: rom.u8(a + 3),
                n_objects: rom.u8(a + 4),
                n_objects2: rom.u8(a + 5),
                flag8: rom.u32(a + 8),
                name_id: rom.u32(a + 0xC),
                cond_objects: rom.u32(a + 0x10),
                objects: rom.u32(a + 0x14),
                objects2: rom.u32(a + 0x18),
                song: rom.u32(a + 0x24),
                word28: rom.u32(a + 0x28),
                headers_list: hl,
                script: rom.u32(a + 0x30),
                word34: rom.u32(a + 0x34),
                headers,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct TileAnim {
    pub addr: u32,
    pub dest_tile: u16,
    pub ntiles: u8,
    /// (duration in frames, LZ blob address) per frame
    pub frames: Vec<(u16, u32)>,
}

#[derive(Clone, Debug)]
pub struct Header {
    pub addr: u32,
    pub bldcnt: u16,
    pub eva: u8,
    pub evb: u8,
    pub priority: [u8; 4],
    pub cam_w: u16,
    pub cam_h: u16,
    pub anims: Vec<TileAnim>,
    pub res: [u32; 4],
    pub n_rects: u32,
    pub rects: u32,
    pub minimap: u32,
    pub word30: u32,
    pub walk: u32,
    pub shot: u32,
    pub mode: u32,
    pub word40: u32,
    pub ptr44: u32,
    pub tile_list: u32,
    pub tile_chunks: u32,
}

impl Header {
    /// World size in px: the camera range + one screen.
    pub fn world_px(&self) -> (u32, u32) {
        (self.cam_w as u32 + 239, self.cam_h as u32 + 159)
    }
}

pub fn header(rom: &Rom, h: u32) -> Header {
    let n_anim = rom.u32(h + 0xC);
    let al = rom.u32(h + 0x10);
    let anims = (0..n_anim)
        .map(|k| {
            let d = rom.u32(al + 4 * k);
            let nf = rom.u8(d);
            TileAnim {
                addr: d,
                ntiles: rom.u8(d + 1),
                dest_tile: rom.u16(d + 2),
                frames: (0..nf as u32)
                    .map(|f| {
                        let p = rom.u32(d + 4 + 4 * f);
                        (rom.u16(p), p + 4)
                    })
                    .collect(),
            }
        })
        .collect();
    Header {
        addr: h,
        bldcnt: rom.u16(h),
        eva: rom.u8(h + 2),
        evb: rom.u8(h + 3),
        priority: [rom.u8(h + 4), rom.u8(h + 5), rom.u8(h + 6), rom.u8(h + 7)],
        cam_w: rom.u16(h + 8),
        cam_h: rom.u16(h + 0xA),
        anims,
        res: [rom.u32(h + 0x14), rom.u32(h + 0x18), rom.u32(h + 0x1C), rom.u32(h + 0x20)],
        n_rects: rom.u32(h + 0x24),
        rects: rom.u32(h + 0x28),
        minimap: rom.u32(h + 0x2C),
        word30: rom.u32(h + 0x30),
        walk: rom.u32(h + 0x34),
        shot: rom.u32(h + 0x38),
        mode: rom.u32(h + 0x3C),
        word40: rom.u32(h + 0x40),
        ptr44: rom.u32(h + 0x44),
        tile_list: rom.u32(h + 0x48),
        tile_chunks: rom.u32(h + 0x4C),
    }
}

/// A BG layer resource.
#[derive(Clone, Debug)]
pub struct Resource {
    pub addr: u32,
    pub kind: u32,
    /// scroll rate (1/1024): layer scroll = (camera * rate + offset) >> 10
    pub rate: (i32, i32),
    /// offset in 1/1024 px
    pub offset: (i32, i32),
    /// RES_WRAP: velocity in 1/1024 px per frame
    pub velocity: (i32, i32),
    pub cols: u8,
    pub rows: u8,
    /// chunk blobs (RES_TILEMAP, row-major) or the single screen (RES_WRAP)
    pub chunks: Vec<u32>,
}

pub fn resource(rom: &Rom, r: u32) -> Option<Resource> {
    if !rom.isptr(r) {
        return None;
    }
    let kind = rom.u32(r);
    let rate = (rom.s32(r + 4), rom.s32(r + 8));
    let offset = (rom.s32(r + 0xC), rom.s32(r + 0x10));
    match kind {
        RES_TILEMAP => {
            let (cols, rows) = (rom.u8(r + 0x14), rom.u8(r + 0x15));
            let chunks = (0..cols as u32 * rows as u32).map(|i| rom.u32(r + 0x18 + 4 * i)).collect();
            Some(Resource { addr: r, kind, rate, offset, velocity: (0, 0), cols, rows, chunks })
        }
        RES_WRAP => Some(Resource {
            addr: r,
            kind,
            // the update (0x08008924) uses +4 for both axes
            rate: (rate.0, rate.0),
            offset,
            velocity: (rom.s32(r + 0x14), rom.s32(r + 0x18)),
            cols: 1,
            rows: 1,
            chunks: vec![rom.u32(r + 0x1C)],
        }),
        _ => Some(Resource { addr: r, kind, rate: (0, 0), offset: (0, 0), velocity: (0, 0), cols: 0, rows: 0, chunks: vec![] }),
    }
}
