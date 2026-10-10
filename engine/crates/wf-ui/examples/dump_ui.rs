//! Write the `ui/` category to a directory: `dump_ui <out dir>` (ROM from $WF_ROM or roms/buu.gba).
fn main() {
    let dir = std::env::args().nth(1).expect("out dir");
    let rom = wf_rom::testutil::rom().expect("ROM not found");
    let mut c = wf_rom::content::Content::new();
    wf_ui::extract(&rom, &mut c).unwrap();
    for (p, d) in &c.files {
        let path = std::path::Path::new(&dir).join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, d).unwrap();
    }
    println!("{} files, {} bytes", c.files.len(), c.total_bytes());
}
