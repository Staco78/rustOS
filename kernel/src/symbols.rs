use core::ffi::CStr;

use alloc::{collections::BTreeMap, string::String};

use crate::fs::open;

static mut SYMBOLS: BTreeMap<String, usize> = BTreeMap::new();

pub fn init() {
    let file = open("/initrd/ksymbols").expect("ksymbols not found").as_file().expect("ksymbols is not a file");
    let symbols = unsafe { &mut SYMBOLS };
    let buff = file.read_to_end_vec(0).unwrap();
    let mut off = 0;
    while off < buff.len() {
        let addr = usize::from_le_bytes(<[u8; 8]>::try_from(&buff[off..off + 8]).unwrap());
        let cstr = CStr::from_bytes_until_nul(&buff[off + 8..]).unwrap();
        let str = String::from_utf8_lossy(cstr.to_bytes());
        symbols.insert(str.into_owned(), addr);
        off += 8 + cstr.to_bytes_with_nul().len();
    }
}

#[inline]
pub fn get(name: &str) -> Option<usize> {
    unsafe { SYMBOLS.get(name).map(|a| *a) }
}
