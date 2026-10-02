//! String handles: the natives that push a *pointer* to a UI string on the GBA (`get_form_name` 0x16,
//! `get_form_label` 0x17, `form_name_ptr` 0xD3, `string_ptr` 0xDE) push a handle here; formatted text nodes
//! (kinds 2 / 6) resolve `%s` arguments through it and print `%d` arguments as integers.

use wf_core::world::World;

const TAG_SHIFT: u32 = 24;
const FORM_NAME: i32 = 0x71;
const FORM_LABEL: i32 = 0x72;
const STRING: i32 = 0x73;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    FormName(i32),
    FormLabel(i32),
    /// UI string table entry (0x0806BCE8)
    String(i32),
}

impl Handle {
    pub fn encode(self) -> i32 {
        match self {
            Handle::FormName(f) => (FORM_NAME << TAG_SHIFT) | (f & 0xffffff),
            Handle::FormLabel(f) => (FORM_LABEL << TAG_SHIFT) | (f & 0xffffff),
            Handle::String(s) => (STRING << TAG_SHIFT) | (s & 0xffffff),
        }
    }
    pub fn decode(v: i32) -> Option<Handle> {
        let x = v & 0xffffff;
        match v >> TAG_SHIFT {
            FORM_NAME => Some(Handle::FormName(x)),
            FORM_LABEL => Some(Handle::FormLabel(x)),
            STRING => Some(Handle::String(x)),
            _ => None,
        }
    }
}

/// the text a handle names (form names from the content; UI strings are not in wf-core's content: `#<id>`)
pub fn resolve(w: &World, v: i32) -> String {
    match Handle::decode(v) {
        Some(Handle::FormName(f)) => w.data.form(f).map(|f| f.name.clone()).unwrap_or_default(),
        Some(Handle::FormLabel(f)) => w.data.form(f).map(|f| f.name.to_uppercase()).unwrap_or_default(),
        Some(Handle::String(s)) => format!("#{s}"),
        None => v.to_string(),
    }
}

/// printf the way 0x0803DB7C does for dialog: `%s` takes a string handle, `%d` an integer, `%%` a percent
pub fn format(w: &World, fmt: &str, args: &[i32]) -> String {
    let mut out = String::new();
    let mut it = fmt.chars().peekable();
    let mut k = 0;
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('s') => {
                out.push_str(&resolve(w, args.get(k).copied().unwrap_or(0)));
                k += 1;
            }
            Some('d') => {
                out.push_str(&args.get(k).copied().unwrap_or(0).to_string());
                k += 1;
            }
            Some('%') => out.push('%'),
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------
// dialogue typewriter: frames per page

static DIALOG_FONT: std::sync::RwLock<Option<std::sync::Arc<Vec<u32>>>> = std::sync::RwLock::new(None);

/// The dialogue font's advance (width + 1) per code unit 0..255 (`wf_ui::font::Font::load(rom, "dialog",
/// 0x0806EE6C)` or content `ui/fonts/dialog.json` "advance"). Needed for the line fitter that decides where the
/// typewriter skips a space. Without it every character counts 6 px.
pub fn set_dialog_font(advance: Vec<u32>) {
    if let Ok(mut f) = DIALOG_FONT.write() {
        *f = Some(std::sync::Arc::new(advance));
    }
}

fn advance(font: &Option<std::sync::Arc<Vec<u32>>>, c: u16) -> u32 {
    match font {
        Some(f) => f.get(c as usize).copied().unwrap_or(1),
        None => 6,
    }
}

/// box style 0 (story text): 4 lines, wrap width 144 (0x08071730)
const WRAP: u32 = 144;
const LINES: usize = 4;

/// the line fitter 0x0803DE6A (same rules as `wf_ui::font::wrap`): leading spaces / new lines skipped, a line ends at
/// `\n` or the end, breaks at the last space whose prefix fits, a word longer than the line is cut
fn wrap(font: &Option<std::sync::Arc<Vec<u32>>>, text: &[u16]) -> Vec<Vec<u16>> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while i < text.len() && matches!(text[i], 0x20 | 0x0A | 0x0B) {
            i += 1;
        }
        if i >= text.len() {
            break;
        }
        let start = i;
        let mut w = 0u32;
        let mut last_space: Option<usize> = None;
        let mut end = text.len();
        let mut next = text.len();
        let mut j = i;
        while j < text.len() {
            let c = text[j];
            if c == 0x0A || c == 0x0B {
                end = j;
                next = j + 1;
                break;
            }
            if c == 8 {
                j += 2;
                continue;
            }
            if c == 0x20 {
                last_space = Some(j);
            }
            let nw = w + advance(font, c);
            if nw > WRAP {
                match last_space {
                    Some(sp) => {
                        end = sp;
                        next = sp + 1;
                    }
                    None => {
                        end = j;
                        next = j;
                    }
                }
                break;
            }
            w = nw;
            j += 1;
        }
        out.push(text[start..end.min(text.len())].to_vec());
        i = next;
    }
    out
}

/// frames the typewriter needs per page: one per visible character of the fitted lines (colour pairs and an
/// option line's "- " marker take none; skipped spaces / new lines / page breaks ride along with the next one)
pub fn typing(text: &str) -> Vec<u16> {
    let font = DIALOG_FONT.read().ok().and_then(|f| f.clone());
    let body = match text.chars().next() {
        Some('!') | Some('@') | Some('#') => &text[1..],
        _ => text,
    };
    let units: Vec<u16> = body.encode_utf16().collect();
    let mut pages = Vec::new();
    for chunk in units.split(|&c| c == 0x0B) {
        let lines = wrap(&font, chunk);
        if lines.is_empty() {
            pages.push(0);
            continue;
        }
        for p in lines.chunks(LINES) {
            let mut n = 0u16;
            for l in p {
                let mut k = 0;
                if l.len() >= 2 && l[0] == '-' as u16 && l[1] == ' ' as u16 {
                    k = 2;
                }
                while k < l.len() {
                    if l[k] == 8 {
                        k += 2;
                        continue;
                    }
                    n += 1;
                    k += 1;
                }
            }
            pages.push(n);
        }
    }
    if pages.is_empty() {
        pages.push(0);
    }
    pages
}

/// script text as the game stores it: `wf_script::rom::decode_utf16` writes control units as `{xxxx}` (page break
/// `{000b}`, colour `{0008}{000n}`); the dialogue box wants the units themselves
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let b = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        if b[i] == b'{' && i + 5 < text.len() && b[i + 5] == b'}' {
            if let Ok(v) = u32::from_str_radix(&text[i + 1..i + 5], 16) {
                if let Some(c) = char::from_u32(v) {
                    out.push(c);
                    i += 6;
                    continue;
                }
            }
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}
