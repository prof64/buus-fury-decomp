//! WebAssembly entry points (no wasm-bindgen; usable from plain JS, see `engine/web/extract.js`).
//!
//! ```text
//! ptr = wf_alloc(rom.length); copy the ROM into memory at ptr
//! status = wf_extract(ptr, rom.length, only_ptr, only_len)   // only = "" or "sprites,maps"
//!   0 = ok, 1 = wrong ROM (wf_result_* holds the error text), 2 = other error
//! bytes = memory[wf_result_ptr() .. + wf_result_len()]       // a POSIX ustar archive of the base pack
//! wf_free_result()
//! ```
//! The archive holds `base/<path>` entries; JS can unpack it in a few lines or hand it to a file-system API.

use std::cell::RefCell;

pub mod core_api;

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn wf_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` must come from [`wf_alloc`] and not be used afterwards (ownership moves here).
#[no_mangle]
pub unsafe extern "C" fn wf_extract(ptr: *mut u8, len: usize, only_ptr: *const u8, only_len: usize) -> u32 {
    let rom = unsafe { Vec::from_raw_parts(ptr, len, len) };
    let only = if only_len == 0 || only_ptr.is_null() {
        String::new()
    } else {
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(only_ptr, only_len) }).into_owned()
    };
    let opts = wf_extract::Options {
        only: only.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
    };
    let (status, out) = match wf_extract::extract(rom, &opts) {
        Ok((content, _)) => {
            let t = tar(&content);
            core_api::remember(content);
            (0, t)
        }
        Err(e @ wf_extract::Error::WrongRom(_)) => (1, e.to_string().into_bytes()),
        Err(e) => (2, e.to_string().into_bytes()),
    };
    RESULT.with(|r| *r.borrow_mut() = out);
    status
}

#[no_mangle]
pub extern "C" fn wf_result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn wf_result_len() -> usize {
    RESULT.with(|r| r.borrow().len())
}

#[no_mangle]
pub extern "C" fn wf_free_result() {
    RESULT.with(|r| *r.borrow_mut() = Vec::new());
}

/// Minimal ustar writer (names up to 255 bytes via the prefix field).
pub fn tar(c: &wf_extract::Content) -> Vec<u8> {
    let mut out = Vec::with_capacity(c.total_bytes() + c.files.len() * 1024 + 1024);
    for (path, data) in &c.files {
        let full = format!("base/{path}");
        let mut h = [0u8; 512];
        let (prefix, name) = if full.len() <= 100 {
            ("", full.as_str())
        } else {
            let cut = full[..full.len() - 100].len();
            let split = full[cut..].find('/').map(|i| cut + i).unwrap_or(0);
            (&full[..split], &full[split + 1..])
        };
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(b"0000644\0");
        h[108..116].copy_from_slice(b"0000000\0");
        h[116..124].copy_from_slice(b"0000000\0");
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn tar_layout() {
        let mut c = wf_extract::Content::new();
        c.put("a.json", b"{}".to_vec());
        let long = format!("maps/{}/collision.json", "x".repeat(120));
        c.put(long.clone(), vec![7; 600]);
        let t = super::tar(&c);
        assert_eq!(t.len() % 512, 0);
        // first entry: base/a.json, size 2
        assert_eq!(&t[..11], b"base/a.json");
        assert_eq!(&t[124..135], b"00000000002");
        // second entry starts after header + one data block; its name is split into prefix + name
        let h = &t[1024..1536];
        let name = String::from_utf8_lossy(&h[..100]).trim_end_matches('\0').to_string();
        let prefix = String::from_utf8_lossy(&h[345..500]).trim_end_matches('\0').to_string();
        assert_eq!(format!("{prefix}/{name}"), format!("base/{long}"));
    }
}
