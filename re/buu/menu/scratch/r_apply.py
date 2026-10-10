R = '/Users/prof/source/buus-fury-decomp/engine/crates/'
p = R + 'wf-render/src/menu.rs'
s = open(p).read()
a = s[s.index("        for &li in &order {\n            let l = &mv.layers[li];"):s.index("        let fb = fade_black.min(16) as u32;")]
b = '''        // the PPU's order: priority 3 down to 0; per priority the BGs (view order: later on top), then the OBJs of
        // that priority (last OAM entry first so index 0 ends on top)
        for p in (0..=3u8).rev() {
            for &li in &order {
                let l = &mv.layers[li];
                if l.priority.min(3) != p {
                    continue;
                }
                let s = &surfaces[li];
                for y in 0..160 {
                    for x in 0..240 {
                        let (sx, sy) = (x - l.x, y - l.y);
                        if sx < 0 || sy < 0 || sx >= s.w || sy >= s.h {
                            continue;
                        }
                        let c = s.px[(sy * s.w + sx) as usize];
                        if c != 0 {
                            let k = (y * 240 + x) as usize;
                            under[k] = (idx[k], top_l[k]);
                            idx[k] = c as u16;
                            top_l[k] = li.min(253) as u8;
                        }
                    }
                }
            }
            for o in mv.objs.iter().rev() {
                if o.priority.min(3) != p {
                    continue;
                }
                let Some(img) = self.image(packs, &o.path) else { continue };
                let (iw, ih) = (img.w as i32, img.h as i32);
                let (fw, fh) = (o.fw as i32, o.fh as i32);
                let per_row = (iw / fw.max(1)).max(1);
                let fx = (o.frame as i32 % per_row) * fw;
                let fy = (o.frame as i32 / per_row) * fh;
                for yy in 0..fh {
                    for xx in 0..fw {
                        let sx = fx + if o.hflip { fw - 1 - xx } else { xx };
                        let sy = fy + if o.vflip { fh - 1 - yy } else { yy };
                        if sx >= iw || sy >= ih {
                            continue;
                        }
                        let c = img.idx[(sy * iw + sx) as usize];
                        let (px, py) = (o.x + xx, o.y + yy);
                        if c == 0 || !(0..240).contains(&px) || !(0..160).contains(&py) {
                            continue;
                        }
                        let k = (py * 240 + px) as usize;
                        under[k] = (idx[k], top_l[k]);
                        idx[k] = c as u16 + o.pal;
                        top_l[k] = 255;
                    }
                }
            }
        }
'''
s = s.replace(a, b, 1)
s = s.replace("        order.sort_by_key(|&i| std::cmp::Reverse(mv.layers[i].priority));\n", "")
# Tile op
a = '''                MenuOp::TextOutline { font, x, y, colour, outline, clip, text } =>'''
assert a in s
s = s.replace(a, '''                MenuOp::Tile { path, tile, x, y, hflip, vflip } => {
                    let Some(img) = self.image(packs, path) else { continue };
                    let per_row = (img.w / 8).max(1) as i32;
                    let (tx, ty) = ((*tile as i32 % per_row) * 8, (*tile as i32 / per_row) * 8);
                    for yy in 0..8 {
                        for xx in 0..8 {
                            let (sx, sy) = (tx + if *hflip { 7 - xx } else { xx }, ty + if *vflip { 7 - yy } else { yy });
                            if sx >= img.w as i32 || sy >= img.h as i32 {
                                continue;
                            }
                            let c = img.idx[(sy * img.w as i32 + sx) as usize];
                            if c != 0 {
                                s.set(x + xx, y + yy, c);
                            }
                        }
                    }
                }
''' + a, 1)
open(p, 'w').write(s)

p = R + 'wf-view/src/lib.rs'
s = open(p).read()
a = '''    AffineObjs { pieces: Vec<[i32; 6]>, pa: i32 },
}'''
assert a in s
s = s.replace(a, '''    AffineObjs { pieces: Vec<[i32; 6]>, pa: i32 },
    /// one 8x8 tile of a tile sheet (`path`: tiles 8x8 left to right, rows top to bottom) at (x, y), index 0 skipped
    Tile { path: String, tile: u16, x: i32, y: i32, hflip: bool, vflip: bool },
}''', 1)
open(p, 'w').write(s)
print('ok')
