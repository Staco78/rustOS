use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("mmu_pages.rs");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest_path)
        .unwrap();
    let page_arr: [String; 512] = (0..512)
        .map(|i| {
            let v = 0x10000000000705u64; // contigous, attrIndex: 1, NS, AP: rw EL1, SH: inner, AF: 1;
            let addr: u64 = i * 0x40000000; // 1GB
            let r = v | addr;
            let str = format!("TableEntry {{ bits: {} }}", r);
            str
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let mut page_arr_str = String::new();
    page_arr_str.push_str("[");
    for (i, entry) in page_arr.iter().enumerate() {
        use std::fmt::Write;
        write!(page_arr_str, "{entry}").unwrap();
        if i != 511 {
            write!(page_arr_str, ", ").unwrap();
        }
    }
    page_arr_str.push_str("]");
    writeln!(
        file,
        "static mut TABLE_LOW: Table = Table({});",
        page_arr_str
    )
    .unwrap();
    writeln!(
        file,
        "static mut TABLE_HIGH: Table = Table([TableEntry {{ bits: 0 }}; 512]);"
    )
    .unwrap();
    writeln!(
        file,
        "static mut TABLE_HIGH_L1_0: Table = Table({});",
        page_arr_str
    )
    .unwrap();
    writeln!(
        file,
        "static mut TABLE_HIGH_L1_511: Table = Table([TableEntry {{ bits: 0 }}; 512]);"
    )
    .unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
