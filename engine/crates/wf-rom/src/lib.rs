//! `wf-rom`: read-only access to a user-supplied *Dragon Ball Z: Buu's Fury* (USA) ROM.
//!
//! * [`Rom`]: verified ROM image with GBA-bus-address accessors.
//! * [`lz`]: the Webfoot LZ decompressor used for nearly every asset.
//! * [`text`]: the UTF-16 string table.
//! * [`gfx`]: GBA colour / tile primitives, a small RGBA image type and PNG encoding.
//! * [`content`]: the in-memory content sink every extractor writes into (path → bytes).
//!
//! Nothing here touches the file system, so the crate builds for `wasm32-unknown-unknown`.

pub mod content;
pub mod gfx;
pub mod lz;
pub mod text;

use std::fmt;

/// SHA-1 of the only supported ROM: Dragon Ball Z - Buu's Fury (USA).
pub const BUU_US_SHA1: &str = "e65738e9d67688309f09811a54f495523ec9aada";

#[derive(Debug)]
pub enum Error {
    /// The ROM is not the supported dump; carries the SHA-1 found.
    WrongRom(String),
    Lz(&'static str),
    OutOfRange(u32),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WrongRom(h) => write!(
                f,
                "unsupported ROM (sha1 {h}); expected Dragon Ball Z - Buu's Fury (USA), sha1 {BUU_US_SHA1}"
            ),
            Error::Lz(m) => write!(f, "webfoot-lz: {m}"),
            Error::OutOfRange(a) => write!(f, "address 0x{a:08X} outside the ROM"),
            Error::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn sha1_hex(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let h = Sha1::digest(data);
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// A verified ROM image. All accessors take GBA bus addresses (`0x08xxxxxx`); the top bits are masked
/// like the Python tools (`a & 0x1FFFFFF`), so plain offsets work too.
pub struct Rom {
    d: Vec<u8>,
    sha1: String,
}

impl Rom {
    /// Verify the SHA-1 and take ownership of the bytes.
    pub fn new(data: Vec<u8>) -> Result<Rom> {
        let h = sha1_hex(&data);
        if h != BUU_US_SHA1 {
            return Err(Error::WrongRom(h));
        }
        Ok(Rom { d: data, sha1: h })
    }

    /// Skip verification (tests on synthetic data only).
    pub fn new_unchecked(data: Vec<u8>) -> Rom {
        Rom { sha1: sha1_hex(&data), d: data }
    }

    pub fn sha1(&self) -> &str {
        &self.sha1
    }
    pub fn data(&self) -> &[u8] {
        &self.d
    }
    pub fn len(&self) -> usize {
        self.d.len()
    }
    pub fn is_empty(&self) -> bool {
        self.d.is_empty()
    }
    #[inline]
    pub fn off(a: u32) -> usize {
        (a & 0x1FF_FFFF) as usize
    }
    #[inline]
    pub fn u8(&self, a: u32) -> u8 {
        self.d.get(Self::off(a)).copied().unwrap_or(0)
    }
    #[inline]
    pub fn s8(&self, a: u32) -> i8 {
        self.u8(a) as i8
    }
    #[inline]
    pub fn u16(&self, a: u32) -> u16 {
        let o = Self::off(a);
        match self.d.get(o..o + 2) {
            Some(b) => u16::from_le_bytes([b[0], b[1]]),
            None => 0,
        }
    }
    #[inline]
    pub fn s16(&self, a: u32) -> i16 {
        self.u16(a) as i16
    }
    #[inline]
    pub fn u32(&self, a: u32) -> u32 {
        let o = Self::off(a);
        match self.d.get(o..o + 4) {
            Some(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            None => 0,
        }
    }
    #[inline]
    pub fn s32(&self, a: u32) -> i32 {
        self.u32(a) as i32
    }
    /// `n` bytes at `a` (clipped to the ROM end).
    pub fn bytes(&self, a: u32, n: usize) -> &[u8] {
        let o = Self::off(a).min(self.d.len());
        &self.d[o..(o + n).min(self.d.len())]
    }
    /// True when `v` is an address inside the ROM image.
    #[inline]
    pub fn isptr(&self, v: u32) -> bool {
        v >= 0x0800_0000 && ((v - 0x0800_0000) as usize) < self.d.len()
    }
    /// Webfoot-LZ decompress the blob at `a`.
    pub fn lz(&self, a: u32) -> Result<Vec<u8>> {
        lz::decompress(&self.d, Self::off(a)).map(|(v, _)| v)
    }
    /// Webfoot-LZ decompress, also returning the bus address just past the stream.
    pub fn lz_end(&self, a: u32) -> Result<(Vec<u8>, u32)> {
        lz::decompress(&self.d, Self::off(a)).map(|(v, e)| (v, e as u32 + 0x0800_0000))
    }
    /// NUL-terminated Latin-1 string.
    pub fn cstr(&self, a: u32, maxlen: usize) -> String {
        self.bytes(a, maxlen).iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
    }
}

/// `0x%08X` formatting used throughout the content files for ROM provenance.
pub fn hx(v: u32) -> String {
    format!("0x{v:08X}")
}

/// Paths used by tests and tools to find the repository's inputs: the ROM (`$WF_ROM` or
/// `<repo>/roms/buu.gba`) and the repository root (parity references in `data/`, `site/public/assets/`).
pub mod testutil {
    use std::path::PathBuf;

    pub fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap_or_default()
    }

    /// The verified ROM, or `None` (tests then skip) when it is not present.
    pub fn rom() -> Option<super::Rom> {
        let p = std::env::var("WF_ROM").map(PathBuf::from).unwrap_or_else(|_| repo_root().join("roms/buu.gba"));
        let d = std::fs::read(p).ok()?;
        super::Rom::new(d).ok()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rom_sha1_and_lz() {
        let Some(r) = super::testutil::rom() else { return };
        assert_eq!(r.sha1(), super::BUU_US_SHA1);
        // collision blob of Southwest Forest 8-3 (header 0x08240044 +0x34) inflates to exactly 0x2000 bytes
        let blob = r.u32(0x0824_0044 + 0x34);
        assert_eq!(r.lz(blob).unwrap().len(), 0x2000);
        // portrait 0: 64x64 8bpp
        let p = r.u32(0x086F_395C);
        assert_eq!(r.lz(p).unwrap().len(), 4096);
    }
}
