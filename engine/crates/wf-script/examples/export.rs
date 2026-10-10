//! Write every decompiled script as Rhai: `cargo run -p wf-script --example export -- <out dir>`
fn main() {
    let rom = wf_rom::testutil::rom().expect("roms/buu.gba");
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, errors) = wf_script::export::export(&rom, &set);
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "out".into()));
    for (p, s) in &files {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    println!("{} files, {} errors", files.len(), errors.len());
    for e in errors {
        println!("  {e:?}");
    }
}
