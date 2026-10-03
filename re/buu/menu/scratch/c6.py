R = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/'
p = R + 'wf-ui/src/menupics/screens.rs'
open(p, 'w').write('''//! Shop, save / load, title, character switch, game over.
//!
//! `ui/menu/title_shade.json`: the 256-entry colour remap the title menus darken their box with (`0x0803F9D4`,
//! table `0x08056A2C`, box (80, 104)-(160, 144)).

use super::MenuPic;
use wf_rom::content::Content;
use wf_rom::Rom;

pub const PICS: &[MenuPic] = &[];

pub const TITLE_SHADE: u32 = 0x0805_6A2C;

pub fn extract(rom: &Rom, out: &mut Content) {
    let t: Vec<u8> = (0..256u32).map(|i| rom.u8(TITLE_SHADE + i)).collect();
    out.put_json(
        "ui/menu/title_shade.json",
        &serde_json::json!({"rom": format!("0x{TITLE_SHADE:08X}"), "code": "0x0803F9D4", "rect": [80, 104, 160, 144],
                            "note": "pixel = table[pixel] over the rect (title / Single Player menus)", "table": t}),
    );
}
''')
p = R + 'wf-ui/src/lib.rs'
s = open(p).read()
if 'screens::extract' not in s:
    s = s.replace("    menupics::opts::extract(rom, out, &pal);\n", "    menupics::opts::extract(rom, out, &pal);\n    menupics::screens::extract(rom, out);\n", 1)
    assert 'screens::extract' in s
    open(p, 'w').write(s)
p = R + 'wf-core/src/menu/shop.rs'
s = open(p).read()
if 'title_shade' not in s:
    s = s.replace("""    pub chapter_code: Vec<u8>,
}""", """    pub chapter_code: Vec<u8>,
    /// the title menus' box remap (`ui/menu/title_shade.json`, title.rs)
    pub title_shade: Vec<u8>,
}""")
    s = s.replace("""        out
    }
    pub fn get(&self""", """        if let Some(b) = src.read("ui/menu/title_shade.json") {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                out.title_shade = v["table"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as u8).collect()).unwrap_or_default();
            }
        }
        out
    }
    pub fn get(&self""")
    open(p, 'w').write(s)
print('ok')
