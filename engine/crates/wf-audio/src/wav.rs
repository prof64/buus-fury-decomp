//! Minimal RIFF WAVE writer / reader.

fn header(out: &mut Vec<u8>, channels: u16, rate: u32, bits: u16, data_len: usize, extra_chunks: usize) {
    let block = channels * bits / 8;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len + extra_chunks) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block as u32).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
}

/// Mono signed 8-bit PCM as an 8-bit WAV (unsigned, +128). `loop_` = (start, end) in samples adds a `smpl`
/// chunk so sample editors / trackers see the loop.
pub fn mono_s8(pcm: &[i8], rate: u32, loop_: Option<(u32, u32)>) -> Vec<u8> {
    let pad = pcm.len() & 1;
    let smpl_len = if loop_.is_some() { 8 + 36 + 24 } else { 0 };
    let mut out = Vec::with_capacity(44 + pcm.len() + pad + smpl_len);
    header(&mut out, 1, rate, 8, pcm.len(), pad + smpl_len);
    out.extend(pcm.iter().map(|&s| (s as i16 + 128) as u8));
    if pad == 1 {
        out.push(0);
    }
    if let Some((s, e)) = loop_ {
        out.extend_from_slice(b"smpl");
        out.extend_from_slice(&60u32.to_le_bytes());
        let period = (1_000_000_000u64 / rate.max(1) as u64) as u32;
        for v in [0u32, 0, period, 60, 0, 0, 0, 1, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        // loop: id, type (0 forward), start, end (inclusive), fraction, play count (0 = infinite)
        for v in [0u32, 0, s, e.saturating_sub(1), 0, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

/// Stereo signed 8-bit pairs as an 8-bit stereo WAV.
pub fn stereo_s8(pcm: &[(i8, i8)], rate: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    header(&mut out, 2, rate, 8, pcm.len() * 2, 0);
    for &(l, r) in pcm {
        out.push((l as i16 + 128) as u8);
        out.push((r as i16 + 128) as u8);
    }
    out
}

/// Interleaved stereo 16-bit WAV.
pub fn stereo_i16(pcm: &[i16], rate: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    header(&mut out, 2, rate, 16, pcm.len() * 2, 0);
    for &s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Parsed WAV: channels, rate, samples as i16 (8-bit data is centred and scaled by 256).
pub struct Wav {
    pub channels: u16,
    pub rate: u32,
    pub samples: Vec<i16>,
}

pub fn read(b: &[u8]) -> Option<Wav> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut p = 12;
    let (mut ch, mut rate, mut bits) = (0u16, 0u32, 0u16);
    while p + 8 <= b.len() {
        let id = &b[p..p + 4];
        let len = u32::from_le_bytes(b[p + 4..p + 8].try_into().ok()?) as usize;
        let body = b.get(p + 8..(p + 8 + len).min(b.len()))?;
        if id == b"fmt " {
            ch = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            let samples = if bits == 16 {
                body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
            } else {
                body.iter().map(|&c| (c as i16 - 128) * 256).collect()
            };
            return Some(Wav { channels: ch, rate, samples });
        }
        p += 8 + len + (len & 1);
    }
    None
}
