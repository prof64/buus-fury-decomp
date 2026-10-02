fn main() {
    let p = std::env::args().nth(1).expect("rom path");
    let rom = std::fs::read(p).unwrap();
    let (d, n) = wf_script_rhai::check::digest(rom, None).unwrap();
    println!("native digest {d:016x} effects {n}");
}
