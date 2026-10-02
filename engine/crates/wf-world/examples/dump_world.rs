//! Extract only the world category (objects, graph, text) to a directory: `dump_world <rom> <out dir> [--maps]`.
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rom = wf_rom::Rom::new(std::fs::read(&args[1]).expect("rom")).expect("verified ROM");
    let mut out = wf_rom::content::Content::new();
    if args.iter().any(|a| a == "--maps") {
        wf_maps::extract(&rom, &mut out).unwrap();
    }
    let t = std::time::Instant::now();
    wf_world::extract(&rom, &mut out).unwrap();
    eprintln!("world: {:?}", t.elapsed());
    for (p, d) in &out.files {
        if !args.iter().any(|a| a == "--maps") || p.ends_with(".json") {
            let f = Path::new(&args[2]).join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, d).unwrap();
        }
    }
    for w in &out.warnings {
        eprintln!("warn: {w}");
    }
}
