use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::Command;

fn main() {
    if env::var("CARGO_FEATURE_KERNEL").is_ok() {
        return;
    }
    println!("cargo:rerun-if-changed=../../build/defs");
    let symbols = fs::read_to_string("../../build/defs").unwrap();

    let mut defs_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("src/defs.rs")
        .unwrap();
    writeln!(defs_file, "extern \"Rust\" {{{}}}", symbols).unwrap();
    defs_file.sync_data().unwrap();
    Command::new("rustfmt")
        .args(["--unstable-features", "--skip-children", "src/defs.rs"])
        .output()
        .unwrap();
}
