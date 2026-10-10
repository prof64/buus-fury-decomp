//! Print the dialogue typewriter frames per page of an event's text nodes (debug aid).
//! `cargo run -p wf-script-core --example typing -- <event>`
fn main() {
    let ev: u16 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(74);
    let rom = wf_rom::testutil::rom().expect("roms/buu.gba");
    let font = wf_ui::font::Font::load(&rom, "dialog", 0x0806_EE6C);
    wf_script_core::text::set_dialog_font((0..256u16).map(|c| font.advance(c)).collect());
    let set = wf_script::ScriptSet::load_all(&rom);
    let e = &set.events[&ev];
    for (k, entry) in &e.entries {
        if let wf_script::rom::Entry::Node(n) = entry {
            if let wf_script::rom::NodeBody::Text { text, .. } | wf_script::rom::NodeBody::Choice { text } = &n.body {
                println!("node {k} kind {}: {:?}\n  typing {:?}", n.kind, text, wf_script_core::text::typing(&wf_script_core::text::unescape(text)));
            }
        }
    }
}
