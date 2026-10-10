//! `cargo run -p wf-maps --example dump_maps --release -- <outdir> [map-id-prefix]`: run the maps extractor on the
//! repository ROM and write the files (for inspection), with timing.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = std::path::PathBuf::from(args.get(1).expect("outdir"));
    let filter = args.get(2).cloned().unwrap_or_default();
    let rom = wf_rom::testutil::rom().expect("ROM");
    let t = std::time::Instant::now();
    let mut c = wf_rom::content::Content::new();
    wf_maps::extract(&rom, &mut c).unwrap();
    eprintln!("{} files, {:.1} MB in {:.2} s", c.files.len(), c.total_bytes() as f64 / 1e6, t.elapsed().as_secs_f64());
    for w in &c.warnings {
        eprintln!("warning: {w}");
    }
    for (p, d) in &c.files {
        if !filter.is_empty() && !p.contains(&filter) && p.matches('/').count() > 1 {
            continue;
        }
        let f = out.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, d).unwrap();
    }
}
