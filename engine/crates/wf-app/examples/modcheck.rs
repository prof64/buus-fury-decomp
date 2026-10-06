//! Mod checker (engine/docs/modding.md §7; the checks are `wf_app::modcheck`): loads `<content/base>` + the given
//! mod folders exactly like the game, lists what each mod touches and reports every problem with its file and a
//! hint. Headless, no window, no audio. The installed game has the same as `buus-fury --modcheck <mod dir>...`.
//!
//! `cargo run -p wf-app --example modcheck -- <content/base> <mod dir>...` (exit code 1 on errors)

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: modcheck <content/base> <mod dir>...");
        std::process::exit(2);
    }
    let mods: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
    let r = wf_app::modcheck::check(&PathBuf::from(&args[0]), &mods);
    print!("{}", r.text());
    std::process::exit(if r.ok() { 0 } else { 1 });
}
