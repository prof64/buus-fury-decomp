//! `blit_scaled` against the old per-pixel form at window sizes (headless): best of 50 runs each.
//!   cargo run --release -p wf-app --example blitbench

use std::time::Instant;

fn old(src: &[u32], sw: usize, sh: usize, scale: usize, dst: &mut [u32], dw: usize, dh: usize) {
    let scale = scale.max(1);
    let (ow, oh) = (sw * scale, sh * scale);
    let ox = (dw as isize - ow as isize) / 2;
    let oy = (dh as isize - oh as isize) / 2;
    for y in 0..dh {
        let row = &mut dst[y * dw..(y + 1) * dw];
        let sy = y as isize - oy;
        if sy < 0 || sy >= oh as isize {
            row.fill(0);
            continue;
        }
        let srow = &src[(sy as usize / scale) * sw..(sy as usize / scale + 1) * sw];
        for (x, d) in row.iter_mut().enumerate() {
            let sx = x as isize - ox;
            *d = if sx < 0 || sx >= ow as isize { 0 } else { srow[sx as usize / scale] & 0x00FF_FFFF };
        }
    }
}

fn best(f: &mut dyn FnMut()) -> f64 {
    (0..50)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::MAX, f64::min)
}

fn main() {
    // (window, the PC view's auto zoom there) and the classic view at its integer scale
    for &(dw, dh, sw, sh, scale) in &[
        (1920usize, 1080usize, 640usize, 360usize, 3usize),
        (2560, 1440, 640, 360, 4),
        (3840, 2160, 640, 360, 6),
        (3840, 2160, 240, 160, 13),
    ] {
        let src: Vec<u32> = (0..sw * sh).map(|i| i as u32 * 2_654_435_761).collect();
        let mut dst = vec![0u32; dw * dh];
        let a = best(&mut || old(&src, sw, sh, scale, &mut dst, dw, dh));
        let b = best(&mut || wf_app::blit_scaled(&src, sw, sh, scale, &mut dst, dw, dh));
        println!("{dw}x{dh} from {sw}x{sh} x{scale}: old {a:.3} ms, new {b:.3} ms ({:.1}x)", a / b);
    }
}
