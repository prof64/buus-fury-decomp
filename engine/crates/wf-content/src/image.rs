//! PNG decoding to the two forms the renderer uses: RGBA (sprite sheets) and indexed + palette (tilesets).

use crate::{err, Error, Result};

#[derive(Clone, Debug)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    /// `0xAARRGGBB` per pixel, row-major.
    pub px: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Indexed {
    pub w: u32,
    pub h: u32,
    pub idx: Vec<u8>,
    /// `0xAARRGGBB` per palette entry (alpha from tRNS; index 0 is transparent in tilesets regardless).
    pub palette: Vec<u32>,
}

fn decoder(bytes: &[u8], t: png::Transformations) -> Result<(png::Reader<std::io::Cursor<&[u8]>>, Vec<u8>)> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(t);
    let mut r = d.read_info().map_err(|e| Error(format!("png: {e}")))?;
    let mut buf = vec![0; r.output_buffer_size()];
    r.next_frame(&mut buf).map_err(|e| Error(format!("png: {e}")))?;
    Ok((r, buf))
}

pub fn decode_rgba(bytes: &[u8]) -> Result<Rgba> {
    let (r, buf) = decoder(bytes, png::Transformations::EXPAND | png::Transformations::STRIP_16)?;
    let info = r.info();
    let (w, h) = (info.width, info.height);
    let (ct, _) = r.output_color_type();
    let n = (w * h) as usize;
    let mut px = Vec::with_capacity(n);
    match ct {
        png::ColorType::Rgba => {
            for c in buf.chunks_exact(4).take(n) {
                px.push(u32::from_be_bytes([c[3], c[0], c[1], c[2]]));
            }
        }
        png::ColorType::Rgb => {
            for c in buf.chunks_exact(3).take(n) {
                px.push(u32::from_be_bytes([255, c[0], c[1], c[2]]));
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for c in buf.chunks_exact(2).take(n) {
                px.push(u32::from_be_bytes([c[1], c[0], c[0], c[0]]));
            }
        }
        png::ColorType::Grayscale => {
            for &c in buf.iter().take(n) {
                px.push(u32::from_be_bytes([255, c, c, c]));
            }
        }
        other => return err(format!("png: unsupported colour type {other:?}")),
    }
    Ok(Rgba { w, h, px })
}

pub fn decode_indexed(bytes: &[u8]) -> Result<Indexed> {
    let (r, buf) = decoder(bytes, png::Transformations::IDENTITY)?;
    let info = r.info();
    if info.color_type != png::ColorType::Indexed {
        return err("png: expected an indexed (palette) image");
    }
    let (w, h) = (info.width, info.height);
    let pal = info.palette.as_deref().unwrap_or(&[]);
    let trns = info.trns.as_deref().unwrap_or(&[]);
    let palette: Vec<u32> = pal
        .chunks_exact(3)
        .enumerate()
        .map(|(i, c)| u32::from_be_bytes([trns.get(i).copied().unwrap_or(255), c[0], c[1], c[2]]))
        .collect();
    let bits = info.bit_depth as u32;
    let stride = (w * bits).div_ceil(8) as usize;
    let mut idx = Vec::with_capacity((w * h) as usize);
    for y in 0..h as usize {
        let row = &buf[y * stride..(y + 1) * stride];
        for x in 0..w as usize {
            let v = match bits {
                8 => row[x],
                4 => (row[x / 2] >> (4 - 4 * (x % 2))) & 15,
                2 => (row[x / 4] >> (6 - 2 * (x % 4))) & 3,
                1 => (row[x / 8] >> (7 - x % 8)) & 1,
                _ => return err("png: unsupported bit depth"),
            };
            idx.push(v);
        }
    }
    Ok(Indexed { w, h, idx, palette })
}
