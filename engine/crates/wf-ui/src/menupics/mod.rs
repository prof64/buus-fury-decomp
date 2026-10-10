//! Pictures the menu screens draw (picture records `{u32 type, u32 size, data}` loaded by `0x0803D940`: type 0 =
//! LZ, else raw), exported to `ui/menu/<id>.png` with the game's raw 8bpp indices (UI palette, index 0
//! transparent), frames stacked vertically. One table per screen family (files in this directory).

pub mod minigame;
pub mod opts;
pub mod pause;
pub mod screens;

pub struct MenuPic {
    pub id: &'static str,
    /// the picture record (of frame 0)
    pub rom: u32,
    pub w: u32,
    pub h: u32,
    /// frames
    pub frames: u32,
    /// bytes between the frames' records (0 = all frames inside the one record)
    pub stride: u32,
    pub desc: &'static str,
}

pub fn all() -> Vec<&'static MenuPic> {
    pause::PICS.iter().chain(screens::PICS.iter()).chain(minigame::PICS.iter()).chain(opts::PICS.iter()).collect()
}
