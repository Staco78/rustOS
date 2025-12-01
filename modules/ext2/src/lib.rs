#![no_std]
#![feature(maybe_uninit_slice)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(int_roundings)]
#![feature(assert_matches)]
#![feature(ptr_metadata)]

extern crate alloc;

mod consts;
mod driver;
mod filesystem;
mod nodes;
mod structs;
mod icache;

use driver::DRIVER;
use kernel::{error::Error, fs};

#[unsafe(no_mangle)]
pub fn init() -> Result<(), Error> {
    fs::register_driver(&DRIVER);
    Ok(())
}
