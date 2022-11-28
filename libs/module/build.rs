use std::{
    fs::{self, OpenOptions},
    io::Write,
};

fn main() {
    println!("cargo:rerun-if-changed=src/defs.rs");
    let file = fs::read_to_string("src/defs.rs").unwrap();
    let mut symbols_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("../../symbols")
        .unwrap();
    let lines = file.lines();
    for line in lines {
        let line = line.trim();
        if line.starts_with("pub fn") {
            let name_end = line.find('(').unwrap();
            let symbol_name = &line[7..name_end];
            writeln!(symbols_file, "{}", symbol_name).unwrap();
        }
    }
}
