use core::ffi::CStr;

use alloc::{collections::BTreeMap, string::String};

use crate::fs;

static mut SYMBOLS: BTreeMap<String, usize> = BTreeMap::new();

pub fn init() {
    let node = fs::get_node("/initrd/ksymbols").expect("ksymbols not found");
    let file = node.as_file().expect("Not a file");
    let symbols = unsafe { &mut *&raw mut SYMBOLS };
    let buff = file.read_to_end_vec(0).unwrap();
    let mut off = 0;
    while off + 10 < buff.len() {
        let addr = usize::from_le_bytes(<[u8; 8]>::try_from(&buff[off..off + 8]).unwrap());
        let cstr = CStr::from_bytes_until_nul(&buff[off + 8..]).unwrap();
        let str = String::from_utf8_lossy(cstr.to_bytes());
        symbols.insert(str.into_owned(), addr);
        off += 8 + cstr.to_bytes_with_nul().len();
    }
}

#[inline]
pub fn get(name: &str) -> Option<usize> {
    unsafe { (&*&raw const SYMBOLS).get(name).copied() }
}
